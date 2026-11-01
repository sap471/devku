<#
Membuat home uji DevKu tanpa mengunduh binary apa pun.

Cache engine diisi sekali dari instalasi nyata di %APPDATA%\DevKu\engines, lalu
setiap home uji memakai hardlink ke cache. Hardlink tidak menyalin isi berkas:
pembuatan home selesai cepat dan tidak menambah pemakaian disk. Menghapus folder
engine di home uji hanya melepas link, isi cache tetap utuh, jadi uji hapus PHP
atau hapus MariaDB tidak merusak cache.

Pemakaian:
  .\scripts\dev\test-home.ps1 -Seed          isi cache dari instalasi nyata
  .\scripts\dev\test-home.ps1                buat home uji bersih
  .\scripts\dev\test-home.ps1 -KeepConfig    pertahankan config.json yang ada

Jangan menjalankan -Seed atau uji pemasangan engine tanpa alasan: itu satu-satunya
jalur yang memakai kuota. Lihat aturan kuota di AGENTS.md.
#>

[CmdletBinding()]
param(
    [string]$HomeDir = (Join-Path $env:LOCALAPPDATA 'Temp\devku-test\home'),
    [string]$Cache = (Join-Path $env:LOCALAPPDATA 'devku-engine-cache'),
    [int]$DatabasePort = 3307,
    [switch]$Seed,
    [switch]$KeepConfig
)

$ErrorActionPreference = 'Stop'

if (-not ('DevkuHardLink' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class DevkuHardLink
{
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateHardLinkW(string link, string target, IntPtr attr);

    public static bool Create(string link, string target)
    {
        return CreateHardLinkW(link, target, IntPtr.Zero);
    }
}
'@
}

function Get-SizeMb([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 0 }
    $bytes = (Get-ChildItem -LiteralPath $Path -Recurse -File -ErrorAction SilentlyContinue |
        Measure-Object Length -Sum).Sum
    return [math]::Round($bytes / 1MB, 1)
}

# Menyusun pohon folder di tujuan lalu membuat satu hardlink per berkas.
function Copy-TreeAsLinks([string]$Source, [string]$Destination) {
    if (-not (Test-Path -LiteralPath $Source)) { return 0 }

    $root = (Resolve-Path -LiteralPath $Source).Path.TrimEnd('\')
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null

    foreach ($dir in Get-ChildItem -LiteralPath $root -Recurse -Directory -Force -ErrorAction SilentlyContinue) {
        $relative = $dir.FullName.Substring($root.Length).TrimStart('\')
        New-Item -ItemType Directory -Force -Path (Join-Path $Destination $relative) | Out-Null
    }

    $made = 0
    foreach ($file in Get-ChildItem -LiteralPath $root -Recurse -File -Force -ErrorAction SilentlyContinue) {
        $relative = $file.FullName.Substring($root.Length).TrimStart('\')
        $link = Join-Path $Destination $relative
        if (Test-Path -LiteralPath $link) { continue }

        $parent = Split-Path -Parent $link
        if (-not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
        }

        # Hardlink hanya bisa di volume yang sama; kalau gagal, salin biasa.
        if (-not [DevkuHardLink]::Create($link, $file.FullName)) {
            Copy-Item -LiteralPath $file.FullName -Destination $link -Force
        }
        $made++
    }
    return $made
}

$liveEngines = Join-Path $env:APPDATA 'DevKu\engines'
$cacheEngines = Join-Path $Cache 'engines'

if ($Seed) {
    if (-not (Test-Path -LiteralPath $liveEngines)) {
        throw "Tidak ada instalasi nyata di $liveEngines. Tidak ada yang bisa di-seed."
    }
    $seeded = Copy-TreeAsLinks $liveEngines $cacheEngines
    "cache diisi dari instalasi nyata: $seeded berkas, $(Get-SizeMb $cacheEngines) MB"
}

if (-not (Test-Path -LiteralPath $cacheEngines)) {
    throw "Cache engine belum ada di $cacheEngines. Jalankan dulu: .\scripts\dev\test-home.ps1 -Seed"
}

if (Test-Path -LiteralPath $HomeDir) {
    if ($KeepConfig) {
        Get-ChildItem -LiteralPath $HomeDir -Force |
            Where-Object { $_.Name -notin @('engines', 'config.json') } |
            Remove-Item -Recurse -Force
    } else {
        Remove-Item -LiteralPath $HomeDir -Recurse -Force
    }
}
New-Item -ItemType Directory -Force -Path $HomeDir | Out-Null

$linked = Copy-TreeAsLinks $cacheEngines (Join-Path $HomeDir 'engines')

if (-not $KeepConfig) {
    $config = @{
        version       = 1
        backend       = 'caddy'
        tld           = 'localhost'
        autostart     = $false
        https_trusted = $false
        onboarded     = $true
        php           = @{ active = ''; children = 4 }
        database      = @{ port = $DatabasePort; autostart = $false }
        sites         = @()
    } | ConvertTo-Json -Depth 6

    # Tanpa BOM: Config::load menolak JSON yang diawali BOM.
    [System.IO.File]::WriteAllText(
        (Join-Path $HomeDir 'config.json'),
        $config,
        [System.Text.UTF8Encoding]::new($false)
    )
}

$php = Get-ChildItem (Join-Path $HomeDir 'engines\php') -Directory -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty Name

"home    : $HomeDir"
"engines : $linked link, $(Get-SizeMb (Join-Path $HomeDir 'engines')) MB"
"php     : $(if ($php) { $php -join ', ' } else { 'tidak ada' })"
"caddy   : $(if (Test-Path (Join-Path $HomeDir 'engines\caddy\caddy.exe')) { 'ada' } else { 'tidak ada' })"
"mariadb : $(if (Test-Path (Join-Path $HomeDir 'engines\mariadb\bin\mysqld.exe')) { 'ada' } else { 'tidak ada' })"
""
"Pakai di sesi ini:"
"  `$env:DEVKU_HOME = '$HomeDir'"
"  `$env:WEBVIEW2_USER_DATA_FOLDER = '$HomeDir\webview'"
""
"Opsional, memblokir semua unduhan supaya uji gagal cepat kalau ada yang lupa:"
"  `$env:HTTPS_PROXY = 'http://127.0.0.1:9'"
