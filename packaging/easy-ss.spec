%global debug_package %{nil}

Name:           easy-ss
Version:        %{app_version}
Release:        1%{?dist}
Summary:        Screenshot editor for GNOME Wayland
License:        LicenseRef-Unspecified
Source0:        easy-ss
Source1:        io.github.easyss.EasySS.desktop
Requires:       gtk4 >= 4.10
Requires:       xdg-desktop-portal
Requires:       xdg-desktop-portal-gnome

%description
Capture a screen, window, or selected area and annotate, copy, or save it as PNG.

%install
install -Dm755 "%{SOURCE0}" "%{buildroot}%{_bindir}/easy-ss"
install -Dm644 "%{SOURCE1}" "%{buildroot}%{_datadir}/applications/io.github.easyss.EasySS.desktop"

%files
%{_bindir}/easy-ss
%{_datadir}/applications/io.github.easyss.EasySS.desktop
