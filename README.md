# floppyformatte

USB disket emülatörü (Gotek / HxC / ipcas-WCH sınıfı) için **çapraz-platform yönetim uygulaması**.
Rust ile yazılmış; FAT12 disket imajları oluşturur/biçimlendirir, blokları (disket slotlarını) yönetir ve görüntüler.

> Tasarım/analiz dokümanı: [`TEKNIK_RAPOR.md`](TEKNIK_RAPOR.md)

## Workspace yapısı

```
crates/
├─ fat12/        Çekirdek: FAT12 imaj formatlayıcı (platformdan bağımsız)
├─ floppyctl/    CLI + blok seti yöneticisi (dosya-tabanlı)
└─ floppy-gui/   Tauri masaüstü arayüzü (Windows/Linux/macOS)
```

### `fat12` (çekirdek kütüphane)
- `Geometry::for_format(FloppyFormat)` — 720K / 1.2M / 1.44M tam BPB geometrisi
- `format_image(format, label, serial)` / `format_image_with(format, &FormatOptions)`
- `add_file(&mut img, path, data)` — imaja dosya ekleme (`fatfs` üzerinden)
- `make_bootable(...)` — boot kodu + sistem dosyalarıyla bootable disket (FreeDOS önerilir)
- Doğrulama: bağımsız `fatfs` crate'iyle round-trip testleri

### `floppyctl` (CLI)
```
floppyctl format --size 1.44 --label MYDISK --out disk.img [--add FILE]
floppyctl info disk.img
floppyctl set-add  --dir <usb_root> --index 0 --size 1.44 --label BOOT [--naming gotek|ipcas]
floppyctl set-list --dir <usb_root> [--naming gotek|ipcas]

# SFR1M44-U100 (Alfais 4408) için ham USB imajı — bankalar 1.5 MiB ofsetlerde
floppyctl build-usb --out usb.img --banks 100 --bank 0:1.44:BOOTDISK --bank 2:1.44:GAMES \
                    --add-to 0:bootfiles/readme.txt
# Sonra: usb.img'i Rufus/Win32DiskImager/dd ile USB'ye yaz, cihaza tak.

# VEYA doğrudan USB'ye yaz (YIKICI, yalnızca removable, Windows, Admin gerekir):
floppyctl list-usb
floppyctl write-usb --image usb.img --index <IDX> --yes
```
- `sfr` — **SFR1M44-U100 (Alfais 4408) ham USB yerleşimi** (bankalar `N×0x180000` ofsette, her banka 1.44MB FAT12). **Bu cihaz için doğru model.**
- `blockset::BlockSet` — dosya-tabanlı emülatörler için (Gotek `DSKA0000.IMG` / ipcas `FLOPPY000.IMG`). *Not: SFR1M44 bunu kullanmaz.*

### `floppy-gui` (Tauri)
- **SFR1M44 USB imajı paneli:** bankaları tanımla → ham `usb.img` üret
- **Doğrudan USB'ye yazma:** removable sürücü seçimi + onaylı yazma (Admin gerekir)
- Blok listesi/formatlama (dosya-tabanlı emülatörler için)
- npm gerektirmez (vanilla HTML + `withGlobalTauri`), saf `cargo` ile derlenir

### `device` (güvenlik katmanı)
- `list_usb_drives()` — yalnızca removable/USB sürücüleri listeler (sistem diski asla)
- `check_write_target()` — removable + boyut doğrulaması (yıkıcı yazma kapısı, test edilir)
- `write_image_to_drive()` — Windows ham yazma: volume kilitle/çıkar + açılan handle'ın cihaz numarasını yeniden doğrula

## Geliştirme

```bash
cargo test            # tüm testler
cargo clippy --all-targets
cargo run -p floppyctl -- info disk.img
cargo run -p floppy-gui        # masaüstü arayüzü (WebView2 gerekir)
```

## Durum
- [x] FAT12 formatlayıcı çekirdeği (720K/1.2M/1.44M) — TDD, round-trip doğrulamalı
- [x] Bootable disk altyapısı (boot kodu + sistem dosyası yerleştirme)
- [x] CLI (format / info / set-add / set-list)
- [x] Dosya-tabanlı blok yöneticisi (Gotek & ipcas adlandırma)
- [x] Tauri GUI iskeleti
- [ ] Gerçek FreeDOS boot dosyalarının paketlenmesi
- [ ] Ham-blok (WCH çipli cihaz) erişimi — donanım modeli doğrulanınca
- [ ] Upload/download, imaj import/export, yedekleme (GUI)
