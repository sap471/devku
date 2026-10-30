# Design Direction

Ditulis dari jawaban pemilik produk. Agen hanya menuangkan; isi arahnya milik pemilik.

## Identitas

Alat kerja harian untuk **developer solo** yang membuka DevKu beberapa kali sehari di sela coding. Bukan untuk tim, bukan untuk pemula yang baru pasang PHP. Prioritasnya cepat dan tidak mengganggu: buka, lihat status, jalankan, tutup.

## Sifat

**Panel kontrol padat.** Banyak informasi terlihat sekaligus: daftar situs, versi PHP, port, status proses, log. Padat bukan berarti berantakan; kepadatan dicapai lewat kolom yang rata dan label yang pendek, bukan lewat tumpukan kartu.

Acuan rasa: VS Code dan Docker Desktop, bukan Linear dan bukan landing page SaaS.

## Dial

```
ENERGY 1 / RHYTHM 2 / MOTION 1
```

- **ENERGY 1**: alat ini tidak menyapa. Tidak ada hero, tidak ada ilustrasi, tidak ada slogan.
- **RHYTHM 2**: satu kerangka konsisten, dengan satu jeda yang disengaja antara daftar situs dan panel detail.
- **MOTION 1**: hanya hover dan transisi status. Tidak ada animasi masuk, tidak ada parallax, tidak ada pulse berulang.

## Palet

Mengikuti tema sistem. Kedua mode wajib diuji, bukan salah satunya.

| Peran | Terang | Gelap |
|---|---|---|
| Latar | `#FAFAF9` | `#0C0D0F` |
| Permukaan (baris, panel) | `#FFFFFF` | `#141619` |
| Garis | `#E4E4E7` | `#26292E` |
| Teks utama | `#18181B` | `#E7E7EA` |
| Teks sekunder | `#52525B` | `#A1A1AA` |
| Aksen | `#B45309` | `#F59E0B` |
| Teks di atas aksen | `#FFFFFF` | `#1C1917` |

**Aturan aksen amber:**

- Amber adalah satu-satunya aksen. Tidak ada warna kedua untuk dekorasi.
- Di mode terang, amber hanya untuk isi tombol dan penanda, **bukan** untuk teks di atas latar terang. Untuk teks, pakai `#B45309`.
- Status proses memakai warna semantik, bukan amber: hidup (hijau), berhenti (netral), gagal (merah). Hijau/merah hanya muncul sebagai titik status, tidak pernah sebagai latar besar.
- Setiap pasangan teks/latar harus lolos WCAG AA 4.5:1 untuk teks normal, 3:1 untuk teks 18px ke atas. Periksa dengan alat, jangan dikira-kira.

## Tipografi

- **Sans** untuk semua teks: `ui-sans-serif, system-ui, "Segoe UI Variable", "Segoe UI", sans-serif`.
- **Mono** hanya untuk data teknis: path, domain, port, versi PHP, PID, isi log. `ui-monospace, "Cascadia Code", "JetBrains Mono", Consolas, monospace`.
- Skala: 12px (label kolom), 13px (isi baris), 15px (judul panel), 20px (judul jendela). Tidak ada teks di bawah 12px.
- Angka memakai `font-variant-numeric: tabular-nums` supaya port dan versi rata saat berubah.

## Kerapatan

- Tinggi baris daftar situs: 40px. Cukup untuk dua baris data tanpa terasa sesak.
- Padding panel: 12px vertikal, 16px horizontal.
- Jarak antar kelompok: 24px. Tidak ada jarak dekoratif di dalam kelompok.
- Lebar kolom tetap, bukan persentase, supaya mata bisa memindai ke bawah.

## Motif identitas

Bilah status setinggi baris di tepi kiri setiap situs, warnanya mengikuti keadaan sebenarnya: hijau (hidup), abu (berhenti), merah (gagal). Bilah ini membawa informasi, bukan hiasan. Tidak ada bilah pada elemen yang tidak punya status.

## Keadaan wajib

Setiap tampilan yang menampilkan data harus punya empat keadaan, dan keempatnya harus dirancang, bukan disusulkan:

1. **Kosong**: belum ada situs. Sebutkan langkah berikutnya, jangan hanya "No data".
2. **Memuat**: saat memindai folder atau menunggu proses siap.
3. **Galat**: sebutkan penyebab dan tindakan berikutnya.
4. **Siap**: keadaan normal.

## Larangan eksplisit untuk proyek ini

Bukan karena aturan umum, tapi karena arah di atas tidak membutuhkannya:

- Gradien sebagai warna utama, glassmorphism, glow, dan bayangan berlapis.
- Kartu bertumpuk untuk setiap situs; satu daftar lebih padat dan lebih mudah dipindai.
- Ikon dekoratif tanpa fungsi.
- Angka atau statistik karangan. Kalau belum ada data nyata, tampilkan kosong.
- Animasi yang berjalan terus tanpa akhir.
