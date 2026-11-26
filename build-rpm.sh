#!/usr/bin/env bash
# Fedora: sudo dnf install cargo gcc gtk4-devel rpm-build
# Jalankan ./build-rpm.sh untuk membuat paket di target/rpm/.
set -euo pipefail
cd -- "$(dirname -- "$0")"

for tool in cargo rpmbuild; do
    command -v "$tool" >/dev/null || { echo "Perintah $tool diperlukan." >&2; exit 1; }
done

cargo build --locked --release --bin easy-ss --target-dir target
package_id=$(cargo pkgid --locked -p easy-ss)
version=${package_id##*[#@]}

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/SOURCES"
cp target/release/easy-ss packaging/io.github.easyss.EasySS.desktop "$work/SOURCES/"
rpmbuild -bb --define "_topdir $work" --define "app_version $version" packaging/easy-ss.spec

mkdir -p target/rpm
rpm_file=("$work"/RPMS/*/easy-ss-*.rpm)
mv -- "${rpm_file[0]}" target/rpm/
echo "Paket tersedia di target/rpm/${rpm_file[0]##*/}"
