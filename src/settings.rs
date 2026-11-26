use gtk::{gio, glib, prelude::*};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Language {
    #[default]
    Indonesian,
    English,
}
impl Language {
    pub fn text<'a>(self, id: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Indonesian => id,
            Self::English => en,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}
impl Theme {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Preferences {
    pub language: Language,
    pub theme: Theme,
}
impl Preferences {
    pub fn load() -> Self {
        let file = glib::KeyFile::new();
        let path = glib::user_config_dir().join("easy-ss/settings.ini");
        if file.load_from_file(path, glib::KeyFileFlags::NONE).is_err() {
            return Self::default();
        }
        let language = if file.string("editor", "language").as_deref() == Ok("en") {
            Language::English
        } else {
            Language::Indonesian
        };
        let theme = Theme::ALL
            .into_iter()
            .find(|t| file.string("editor", "theme").as_deref() == Ok(t.name()))
            .unwrap_or_default();
        Self { language, theme }
    }
    pub fn save(self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = glib::user_config_dir().join("easy-ss");
        std::fs::create_dir_all(&dir)?;
        let file = glib::KeyFile::new();
        file.set_string("editor", "language", self.language.text("id", "en"));
        file.set_string("editor", "theme", self.theme.name());
        file.save_to_file(dir.join("settings.ini"))?;
        Ok(())
    }
}

pub fn theme_handler(settings: gtk::Settings, choice: Rc<Cell<Theme>>) -> impl Fn() {
    let scheme = "gtk-interface-color-scheme";
    let desktop = if settings.find_property(scheme).is_none() {
        gio::SettingsSchemaSource::default()
            .and_then(|s| s.lookup("org.gnome.desktop.interface", true))
            .map(|s| gio::Settings::new_full(&s, None::<&gio::SettingsBackend>, None))
    } else {
        None
    };
    if let Some(desktop) = &desktop {
        let choice = choice.clone();
        let settings = settings.clone();
        desktop.connect_changed(Some("color-scheme"), move |d, _| {
            if choice.get() == Theme::System {
                settings.set_gtk_application_prefer_dark_theme(
                    d.string("color-scheme") == "prefer-dark",
                );
            }
        });
    }
    move || {
        let theme = choice.get();
        if let Some(property) = settings.find_property(scheme) {
            if theme == Theme::System {
                settings.reset_property(scheme);
            } else if let Some(value) = glib::EnumClass::with_type(property.value_type())
                .and_then(|e| e.to_value_by_nick(theme.name()))
            {
                settings.set_property_from_value(scheme, &value);
            }
        } else if theme == Theme::System && desktop.is_none() {
            settings.reset_property("gtk-application-prefer-dark-theme");
        } else {
            settings.set_gtk_application_prefer_dark_theme(
                theme == Theme::Dark
                    || theme == Theme::System
                        && desktop
                            .as_ref()
                            .is_some_and(|d| d.string("color-scheme") == "prefer-dark"),
            );
        }
    }
}

#[derive(Default)]
pub struct Labels(Vec<Box<dyn Fn(Language)>>);
impl Labels {
    pub fn bind(&mut self, f: impl Fn(Language) + 'static) {
        self.0.push(Box::new(f))
    }
    pub fn apply(&self, language: Language) {
        for f in &self.0 {
            f(language)
        }
    }
    pub fn dropdown(
        &mut self,
        id: &'static [&'static str],
        en: &'static [&'static str],
    ) -> gtk::DropDown {
        let dropdown = gtk::DropDown::from_strings(id);
        let target = dropdown.clone();
        self.bind(move |l| {
            let selected = target.selected();
            let strings = if l == Language::English { en } else { id };
            target.set_model(Some(&gtk::StringList::new(strings)));
            target.set_selected(selected);
        });
        dropdown
    }
    pub fn label(&mut self, id: &'static str, en: &'static str) -> gtk::Label {
        let label = gtk::Label::new(None);
        let target = label.clone();
        self.bind(move |l| target.set_text(l.text(id, en)));
        label
    }
    pub fn tip(&mut self, widget: &impl IsA<gtk::Widget>, id: &'static str, en: &'static str) {
        let widget = widget.as_ref().clone();
        self.bind(move |l| {
            let text = l.text(id, en);
            widget.set_tooltip_text(Some(text));
            widget.update_property(&[gtk::accessible::Property::Label(text)]);
        });
    }
    pub fn button(&mut self, icon: &str, id: &'static str, en: &'static str) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        row.append(&gtk::Image::from_icon_name(icon));
        row.append(&self.label(id, en));
        row
    }
}
