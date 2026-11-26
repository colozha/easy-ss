# Easy SS

Gunakan [AGENTS.md](AGENTS.md) sebagai panduan kerja utama. Baca peta kode, batas ruang lingkup, dan langkah verifikasi di sana sebelum mengedit. Lihat [README.md](README.md) untuk dependensi Fedora, cara menjalankan aplikasi, dan pengemasan RPM.

## Perintah saat mengerjakan proyek

```bash
cargo run --locked
cargo fmt --all -- --check
cargo test --locked
```

Untuk menelusuri alur tangkapan layar, `Editor::start` di `src/main.rs` memilih mode dan jeda, lalu memanggil `capture::capture`. `--capture-rectangle` memaksa mode persegi panjang tanpa jeda pilihan pengguna dan juga dipakai oleh pintasan GNOME. Saat menelusuri hasil edit, `Document::pixels` menyediakan gambar untuk penyalinan dan penyimpanan PNG. Tes unit di `src/document.rs` dapat dijalankan tanpa membuka jendela aplikasi.
