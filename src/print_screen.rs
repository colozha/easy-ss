use crate::settings::Language;
use gtk::{gio, glib, prelude::*};
use std::{error::Error, path::PathBuf};

const SHELL: &str = "org.gnome.shell.keybindings";
const PICKER: &str = "show-screenshot-ui";
const MEDIA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const SHORTCUTS: &str = "custom-keybindings";
const PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/easy-ss/";
const KEY: &str = "Print";
pub const CAPTURE_OPTION: &str = "capture-rectangle";

pub struct PrintScreen {
    shell: gio::Settings,
    media: gio::Settings,
    custom_schema: gio::SettingsSchema,
    shortcut: gio::Settings,
    backup: PathBuf,
}

impl PrintScreen {
    pub fn open() -> Option<Self> {
        if !std::env::var("XDG_CURRENT_DESKTOP")
            .ok()?
            .split(':')
            .any(|desktop| desktop == "GNOME")
        {
            return None;
        }
        let source = gio::SettingsSchemaSource::default()?;
        let shell = source.lookup(SHELL, true)?;
        let media = source.lookup(MEDIA, true)?;
        let custom_schema = source.lookup(CUSTOM, true)?;
        if !shell.has_key(PICKER)
            || !media.has_key(SHORTCUTS)
            || !["name", "command", "binding"]
                .into_iter()
                .all(|key| custom_schema.has_key(key))
        {
            return None;
        }
        let settings =
            |schema, path| gio::Settings::new_full(schema, None::<&gio::SettingsBackend>, path);
        Some(Self {
            shell: settings(&shell, None),
            media: settings(&media, None),
            shortcut: settings(&custom_schema, Some(PATH)),
            custom_schema,
            backup: glib::user_config_dir().join("easy-ss/print-screen-backup"),
        })
    }

    pub fn enabled(&self) -> bool {
        self.media.strv(SHORTCUTS).contains(PATH)
            && self.shortcut.string("binding") == KEY
            && self
                .shortcut
                .string("command")
                .ends_with(" --capture-rectangle")
    }

    pub fn set_enabled(&self, enabled: bool, language: Language) -> Result<(), Box<dyn Error>> {
        let mut paths: Vec<String> = self.media.get(SHORTCUTS);
        let registered = paths.iter().any(|path| path == PATH);
        let previous = self.shell.user_value(PICKER);
        let current: Vec<String> = self.shell.get(PICKER);

        if !self.media.is_writable(SHORTCUTS)
            || !self.shell.is_writable(PICKER)
            || !["name", "command", "binding"]
                .into_iter()
                .all(|key| self.shortcut.is_writable(key))
        {
            return Err(language
                .text(
                    "Pengaturan pintasan GNOME terkunci.",
                    "GNOME shortcut settings are locked.",
                )
                .into());
        }

        if enabled {
            let print = gtk::accelerator_parse(KEY);
            for path in paths.iter().filter(|path| *path != PATH) {
                let shortcut = gio::Settings::new_full(
                    &self.custom_schema,
                    None::<&gio::SettingsBackend>,
                    Some(path),
                );
                if gtk::accelerator_parse(shortcut.string("binding")) == print {
                    return Err(language
                        .text(
                            "Print Screen sudah dipakai pintasan kustom lain.",
                            "Print Screen is already assigned to another custom shortcut.",
                        )
                        .into());
                }
            }
            let executable = glib::shell_quote(std::env::current_exe()?);
            let executable = executable
                .to_str()
                .ok_or("The Easy SS executable path is not valid UTF-8.")?;
            let command = format!("{executable} --{CAPTURE_OPTION}");

            if !registered || current.iter().any(|binding| binding == KEY) {
                std::fs::create_dir_all(self.backup.parent().unwrap())?;
                let backup = (current.clone(), previous.is_some()).to_variant();
                glib::file_set_contents(&self.backup, backup.print(true).as_bytes())?;
            }

            // Release the shell binding before GNOME registers the custom shortcut.
            let remaining: Vec<_> = current
                .iter()
                .filter(|binding| *binding != KEY)
                .cloned()
                .collect();
            if remaining != current {
                self.shell.set(PICKER, &remaining)?;
                gio::Settings::sync();
            }
            self.shortcut.delay();
            let result = (|| {
                self.shortcut.set_string("name", "Easy SS")?;
                self.shortcut.set_string("command", &command)?;
                self.shortcut.set_string("binding", KEY)?;
                if !registered {
                    paths.push(PATH.to_owned());
                    self.media.set(SHORTCUTS, &paths)?;
                }
                Ok::<_, glib::BoolError>(())
            })();
            if let Err(err) = result {
                self.shortcut.revert();
                if remaining != current {
                    self.write_picker(previous.as_ref())?;
                }
                return Err(err.into());
            }
            self.shortcut.apply();
        } else {
            let restored = self.restored_picker(&current)?;
            let remaining: Vec<_> = paths.iter().filter(|path| *path != PATH).cloned().collect();
            self.media.set(SHORTCUTS, &remaining)?;
            gio::Settings::sync();
            if let Some(restored) = restored
                && let Err(err) = self.write_picker(restored.as_ref())
            {
                self.media.set(SHORTCUTS, &paths)?;
                return Err(err.into());
            }
            self.shortcut.delay();
            for key in ["name", "command", "binding"] {
                self.shortcut.reset(key);
            }
            self.shortcut.apply();
        }
        gio::Settings::sync();
        Ok(())
    }

    fn write_picker(&self, value: Option<&glib::Variant>) -> Result<(), glib::BoolError> {
        if let Some(value) = value {
            self.shell.set_value(PICKER, value)?;
        } else {
            self.shell.reset(PICKER);
        }
        Ok(())
    }

    fn restored_picker(
        &self,
        current: &[String],
    ) -> Result<Option<Option<glib::Variant>>, Box<dyn Error>> {
        let backup = match std::fs::read_to_string(&self.backup) {
            Ok(backup) => backup,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let (original, had_user_value): (Vec<String>, bool) =
            glib::Variant::parse(Some(glib::VariantTy::new("(asb)")?), &backup)?
                .get()
                .ok_or("Invalid Print Screen backup.")?;
        if !original.iter().any(|binding| binding == KEY)
            || current.iter().any(|binding| binding == KEY)
        {
            return Ok(None);
        }
        let without_print = |bindings: &[String]| {
            bindings
                .iter()
                .filter(|binding| *binding != KEY)
                .cloned()
                .collect::<Vec<_>>()
        };
        if current == without_print(&original) {
            return Ok(Some(had_user_value.then(|| original.to_variant())));
        }
        let mut merged = current.to_vec();
        merged.push(KEY.to_owned());
        Ok(Some(Some(merged.to_variant())))
    }
}
