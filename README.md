# Easy SS

Easy SS adalah aplikasi untuk mengambil dan mengedit tangkapan layar di GNOME Wayland. Aplikasi ini ditulis dengan Rust dan GTK 4, serta memakai XDG Desktop Portal untuk mengambil gambar.

## Persyaratan

- Sesi GNOME Wayland dengan `xdg-desktop-portal` dan `xdg-desktop-portal-gnome`.
- GTK 4.10 atau lebih baru.
- Untuk build dari sumber, gunakan toolchain Rust yang mendukung edisi 2024, linker C, `pkg-config`, header pengembangan GTK 4, dan `glib-compile-resources`.

## Menjalankan dari sumber

Di Fedora, instal dependensi yang diperlukan lalu jalankan aplikasi dari direktori proyek.

```bash
sudo dnf install cargo gcc pkgconf-pkg-config gtk4-devel glib2-devel xdg-desktop-portal xdg-desktop-portal-gnome
cargo run --locked
```

Gunakan `cargo run --locked -- --capture` untuk langsung mengambil tangkapan layar dengan mode dan jeda yang aktif. Gunakan `cargo run --locked -- --capture-rectangle` untuk langsung memilih area persegi panjang tanpa jeda pilihan pengguna.

## Penggunaan

1. Pilih mode **Persegi panjang**, **Bentuk bebas**, atau **Layar penuh**. Pilih jeda tanpa hitung mundur, 3 detik, 5 detik, atau 10 detik.
2. Klik **Baru**. Untuk mode area, seret untuk memilih persegi panjang atau gambar batas bentuk bebas. Pemilihan bisa dilakukan di monitor mana pun. Tekan `Esc` untuk membatalkan pemilihan area.
3. Gunakan **Pena**, **Stabilo**, **Penghapus**, dan **Potong** untuk mengedit gambar. Alat **Pilih** dapat memindahkan atau mengubah ukuran coretan yang dipilih.
4. Hasil tangkapan disalin otomatis saat jendela editor mendapat fokus. Klik **Salin** untuk menyalin hasil edit terbaru atau **Simpan** untuk menyimpannya sebagai PNG.

| Pintasan | Tindakan |
| --- | --- |
| `Ctrl+N` | Ambil tangkapan layar baru |
| `Ctrl+C` | Salin gambar ke clipboard |
| `Ctrl+S` | Simpan gambar sebagai PNG |
| `Ctrl+Z` | Urungkan perubahan |
| `Ctrl+Shift+Z` | Ulangi perubahan |
| `Delete` | Hapus coretan yang dipilih |

Di **Pengaturan**, pilih bahasa Indonesia atau Inggris dan tema sistem, terang, atau gelap. Opsi **Simpan tangkapan** menentukan lokasi berkas hasil tangkapan: **Sementara (/tmp)** (bawaan, terhapus saat komputer dimulai ulang) atau **Gambar/Screenshots** (permanen). Opsi **Gunakan Print Screen** mengalihkan tombol `Print Screen` GNOME ke mode persegi panjang Easy SS. Matikan opsi ini sebelum menghapus aplikasi agar Easy SS dapat memulihkan pintasan GNOME sebelumnya.

## Membuat dan memasang RPM

Dengan dependensi build di atas terpasang, jalankan di Fedora.

```bash
sudo dnf install rpm-build
./build-rpm.sh
```

Paket dibuat di `target/rpm/`. Untuk memasang paket, gunakan `sudo dnf install` dengan path `.rpm` yang dicetak oleh skrip.

## Pemeriksaan pengembangan

```bash
cargo fmt --all -- --check
cargo test --locked
```

Tes unit mencakup pemilihan area, pemetaan koordinat, rendering coretan, penghapusan, dan riwayat edit. Alur portal, clipboard, dan pintasan global dapat diperiksa langsung dalam sesi GNOME Wayland.
