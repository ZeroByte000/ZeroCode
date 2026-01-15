# ZeroCode

CLI code editor ringan berbasis Rust.

## Menjalankan

```bash
cargo run -p zerocode
```

## Mode

- Normal: navigasi dan perintah.
- Insert: mengetik teks ke buffer.
- Command: input prompt untuk buka file, save as, search, dan go to line.

## Perintah Utama

- `q`: keluar
- `i`: masuk ke Insert mode
- `Esc`: kembali ke Normal mode
- `Ctrl+Left`: fokus ke panel file
- `Tab`: fokus ke panel file (alternatif)
- `Ctrl+B`: toggle panel kiri
- `Ctrl+L`: cari file di panel kiri
- `Ctrl+E`: expand semua folder di panel kiri
- `Ctrl+W`: collapse semua folder di panel kiri
- `Ctrl+P`: buka file dari path
- `Ctrl+S`: simpan file (jika belum ada path akan minta Save As)
- `Ctrl+Z`: undo
- `Ctrl+Y`: redo
- `Ctrl+F`: search
- `Ctrl+G`: go to line

## Navigasi

- Panah `Up/Down/Left/Right` atau `h/j/k/l` pada Normal mode.
- Saat fokus panel file: `Up/Down` pilih file, `Enter` buka/expand, `Esc` kembali ke editor.
- Saat pencarian file aktif, daftar akan terfilter otomatis saat mengetik.

## Command Mode

- Muncul di status bar bawah: `Open:`, `Save as:`, `Search:`, `Go to line:`
- Ketik input, tekan `Enter` untuk eksekusi
- Tekan `Esc` untuk batal

## Catatan

- Fitur masih tahap awal: belum ada syntax highlight, LSP, split view, atau tree file.
- Search saat ini mencari kemunculan pertama dari awal dokumen.
- Panel file menampilkan tree folder kerja (rekursif, dengan expand/collapse).
- Ada item `..` untuk naik satu level folder.
- File aktif disorot hijau, seleksi panel disorot abu-abu.
- Ikon: `[+]` folder tertutup, `[-]` folder terbuka, `[F]` file, `[..]` naik level.
- Highlight sederhana untuk `.rs`, `.js/.ts`, `.json`, `.py`, `.go` (keyword, string multi-line, angka, komentar, operator).
- Jika shortcut `Ctrl+...` diblok terminal, gunakan `Alt+P/S/F/G/Z/Y` atau alternatif di mode Normal: `o` open, `s` save, `/` search, `g` goto, `f` file search, `b` toggle panel.
