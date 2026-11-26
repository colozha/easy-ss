# Panduan agen untuk Easy SS

## Ruang lingkup perubahan

- Sebelum mengedit, baca panduan direktori dan implementasi terkait. Identifikasi berkas serta fungsi yang harus diubah, lalu cocokkan rencana dengan perubahan yang diminta dan batasan yang diberikan.
- Pertahankan perilaku di luar permintaan. Hindari refaktor, perubahan dependensi, fitur tambahan, dan pemformatan yang tidak terkait.
- Setelah mengedit, tinjau seluruh diff, pastikan semua perubahan yang diminta terpenuhi, dan perbaiki setiap perubahan di luar ruang lingkup.

## Peta kode

| Lokasi | Tanggung jawab |
| --- | --- |
| `src/main.rs` | Aplikasi GTK, tindakan editor, gestur kanvas, clipboard, dialog simpan, dan opsi CLI |
| `src/capture.rs` | Permintaan ke XDG Desktop Portal, penanganan izin GNOME, pemuatan gambar hasil tangkapan, dan folder penyimpanan |
| `src/record.rs` | Sesi portal ScreenCast dan perekaman video WebM melalui `gst-launch-1.0` |
| `src/document.rs` | Geometri, rendering Cairo, anotasi, crop, riwayat edit, dan tes unit |
| `src/settings.rs` | Preferensi bahasa dan tema, penyimpanan `easy-ss/settings.ini`, serta label UI |
| `src/print_screen.rs` | Pendaftaran pintasan `Print Screen` GNOME dan pemulihan pintasan sebelumnya |
| `build.rs`, `icons/` | Kompilasi ikon toolbar menjadi GResource |
| `build-rpm.sh`, `packaging/` | Build RPM dan entri menu aplikasi |

## Konvensi proyek

- Saat menambah atau mengubah teks UI, perbarui pasangan bahasa Indonesia dan Inggris melalui `Language` atau `Labels` yang sudah ada. Sesuaikan label aksesibilitas jika kontrol terkait berubah.
- Untuk perubahan editor, gunakan pemetaan koordinat dan riwayat edit yang sudah ada di `Document`, `Rect`, dan `View`. Periksa konsistensi antara pratinjau kanvas, clipboard, dan PNG jika perubahan memengaruhi rendering.
- Untuk perubahan tangkapan layar atau `Print Screen`, periksa pembatalan portal dan pemulihan pintasan yang relevan. Uji pengaturan GNOME melalui opsi aplikasi agar jalur pengguna yang sama ikut diperiksa.

## Verifikasi

- Jalankan `cargo fmt --all -- --check` dan `cargo test --locked` untuk perubahan Rust.
- Untuk perubahan pengemasan, jalankan `bash -n build-rpm.sh` dan verifikasi build dengan `./build-rpm.sh` di Fedora.
- Untuk perubahan yang bergantung pada desktop, uji alur terkait di GNOME Wayland dan sebutkan pemeriksaan manual yang belum dapat dilakukan.
