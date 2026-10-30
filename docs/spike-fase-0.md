# Hasil Spike Fase 0

Tanggal: 2026-10-30
Mesin: Windows 11 build 26200, user non-elevated (`DESKTOP-RQLKUBV\aku`)
Binari: Caddy 2.11.7, PHP 8.3.36 NTS, PHP 7.4.33 NTS

Semua spike dijalankan dengan user **tanpa hak admin**.

## Ringkasan

| Spike | Pertanyaan | Hasil |
|---|---|---|
| A | php-cgi + Caddy `php_fastcgi` bisa jadi front controller? | **Lulus** |
| B | Dua versi PHP paralel di dua domain? | **Lulus** |
| C | Konkurensi php-cgi di Windows? | **Lulus, dengan koreksi** |
| D | Tulis hosts butuh elevasi? | **Terkonfirmasi butuh admin — tapi bisa dihindari** |

---

## Spike A — Caddy + php-cgi + front controller

`php-cgi.exe -b 127.0.0.1:9001` + Caddy `php_fastcgi 127.0.0.1:9001`.

```
GET /             -> 200, SCRIPT_NAME=/index.php, REQUEST_URI=/
GET /users/123    -> 200, SCRIPT_NAME=/index.php, REQUEST_URI=/users/123
GET /nope.php     -> 200 (rewrite ke index.php, sama seperti perilaku .htaccess Laravel)
```

`php_fastcgi` sudah memberi `try_files` + front controller. Tidak perlu `rewrite` manual.

**Kesimpulan:** Laravel/CI4 jalan tanpa `.htaccess`, tanpa konfigurasi tambahan per framework.

---

## Spike B — Multi-PHP per domain

Dua pool `php-cgi` (9001 = 8.3.36, 9002 = 7.4.33) dan dua blok Caddy:

```
app-a.localhost:8080 -> php_fastcgi 127.0.0.1:9001 -> PHP 8.3.36
app-b.localhost:8081 -> php_fastcgi 127.0.0.1:9002 -> PHP 7.4.33
```

Keduanya melayani request bersamaan, versi terisolasi, ekstensi (`mbstring`) aktif di masing-masing.

**Reload tanpa downtime:** `POST /load` ke admin API Caddy (`:2019`) menerapkan config baru tanpa restart proses. Site lain tidak terganggu.

**Kesimpulan:** syarat multi-PHP per domain terpenuhi dengan satu proses Caddy.

---

## Spike C — Konkurensi php-cgi di Windows

Asumsi awal di plan ("`PHP_FCGI_CHILDREN` tidak berlaku di Windows") **salah untuk build ini**.

### Bukti

| Pengaturan | Proses hidup | 2 request `/slow` (2s sleep) |
|---|---|---|
| tanpa `PHP_FCGI_CHILDREN` | 1 | 4193 ms (serial) |
| `PHP_FCGI_CHILDREN=4` | 5 (1 induk + 4 anak) | 2091 ms (paralel, PID berbeda) |

- `PHP_FCGI_CHILDREN=4` → 6 request paralel selesai 4216 ms ≈ 2 batch. Batas pool = nilai env, sisanya antre. Perilaku **predictable**.
- Anak yang dimatikan **direspawn otomatis** oleh induk (5 proses kembali, `200`).
- **Induk dimatikan → seluruh pool mati** (0 proses, `502`).

### Konsekuensi untuk desain

1. **Tidak perlu** banyak port + load balancing. Satu `php-cgi` per versi dengan `PHP_FCGI_CHILDREN` sudah cukup.
2. **Wajib ada supervisor** di DevKu: pantau PID induk, restart saat exit. Ini titik kegagalan tunggal per versi PHP.
3. Ukuran pool jadi setelan per versi (default 4).

---

## Spike D — Hosts & elevasi

- Menulis `C:\Windows\System32\drivers\etc\hosts` sebagai user biasa → `UnauthorizedAccessException`. **Elevasi diperlukan.**
- User `aku` ada di grup Administrators, jadi UAC prompt akan muncul (tidak diuji otomatis karena interaktif).

### Temuan yang membuat ini sebagian besar tidak relevan

**Windows 11 build 26200 me-resolve `*.localhost` ke loopback secara liar**, tanpa entri hosts:

```
app-b.localhost    -> 127.0.0.1
my-app.localhost   -> 127.0.0.1
a.b.localhost      -> 127.0.0.1
xyz123.localhost   -> 127.0.0.1
```

Diverifikasi end-to-end: `curl http://app-a.localhost:8080/deep/route` berhasil **tanpa `--resolve` dan tanpa menyentuh hosts**.

`.test` **tidak** resolve tanpa hosts (sesuai dugaan).

### Konsekuensi

- **TLD default harus `localhost`, bukan `test`.** Nol setup, nol UAC, nol risiko merusak hosts.
- `.localhost` tetap aman: RFC 6761, tidak pernah bocor ke DNS publik.
- Hosts editor turun status jadi **fallback** — hanya untuk user yang memilih TLD custom (`.test`, `.dev`, dll).
- Deteksi saat first run: resolve `devku-probe.localhost`; kalau gagal (Windows 10 lama), baru tawarkan hosts + elevasi.

---

## Bonus — Port 80 tanpa admin

`bind 127.0.0.1:80` dan `bind 0.0.0.0:80` **berhasil tanpa elevasi**. Caddy melayani `http://port80.localhost` dengan benar.

Windows tidak memakai model "privileged port" seperti Unix. Risiko yang tersisa hanya `http.sys` reservation (IIS) atau proses lain yang sudah memegang port.

---

## Perubahan yang harus masuk ke plan

1. TLD default → `localhost`; hosts editor jadi fallback opsional.
2. Hapus kebutuhan multi-port per versi; pakai `PHP_FCGI_CHILDREN`.
3. Tambah komponen **supervisor** untuk PID induk `php-cgi`.
4. Risiko UAC turun drastis; risiko baru: induk `php-cgi` mati = pool mati.
