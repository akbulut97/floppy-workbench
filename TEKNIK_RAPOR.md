# USB Floppy Emulator Yönetim Yazılımı — Teknik Analiz ve Geliştirme Raporu

[English](TECHNICAL_REPORT.md) · **Türkçe**

**Referans uygulama:** ipcas GmbH *USB Floppy Emulator V2 (FDEv2)* v1.40i
**Gerçek motor:** WCH / WinChipHead "SFD UFloppyManager II" (rebrand)
**Rapor tarihi:** 2026-11-16
**Amaç:** Benzer bir USB disket emülatör yönetim uygulaması geliştirmek için referans/şartname

> Not: Bu rapordaki "referans uygulama" bulguları **statik analize** (dosya yapısı + ikili dosyadan çıkarılan ASCII/Unicode string'leri, kaynak adları, registry/dosya düzeni) dayanır. Uygulama çalıştırılarak davranışsal doğrulama yapılmamıştır; "doğrulanması gereken" olarak işaretli maddeler çalışma zamanında teyit edilmelidir.

---

## 1. Amaç ve Kapsam

Geliştirilecek uygulama, bir **USB disket emülatörü** (Gotek/HxC sınıfı donanım veya WCH tabanlı USB disket aygıtı) içindeki USB belleği yönetir. Temel işlev:

- USB belleği, her biri **bağımsız bir disketi taklit eden** çok sayıda "blok"a bölmek,
- Bu blokları FAT12 disket formatında biçimlendirmek,
- Blokları listelemek/görüntülemek, içerik aktarmak (upload/download), imaj yazmak/kaydetmek, yedeklemek.

Hedef makine (ör. eski CNC, tekstil makinesi, tıbbi cihaz, sentezleyici) emülatöre tek seferde **bir bloğu** fiziksel 3.5" disket (A:) gibi görür.

---

## 2. Referans Uygulama Analizi

### 2.1 Bileşenler

| Dosya | Rol |
|---|---|
| `USB_Floppy_Manager_v140i.exe` | GUI (PE32, i386, MFC tabanlı, ~590 KB) |
| `QHSFDSvc.exe` | Windows servisi: `WCHSFDService` ("floppy disk block service") |
| `psvince.dll` | Kurulum/kopya-koruma yardımcısı (çalışan süreç kontrolü) |
| `FDEv2_*.pdf` | Kullanıcı kılavuzları (DE/EN) |

### 2.2 Mimari

- **İki katman:** GUI ↔ çekirdek servis.
- Servis **yönetici (admin)** hakkı ve **Windows 2000+** gerektirir (`Un-support NT system, run above of Win2000`). Servis kurulum/kaldırma işlemi uygulama içinden yapılır.
- **IPC (süreçler arası iletişim):** Paylaşımlı bellek + senkronizasyon nesneleri:
  - `Global\WCHSFDMappingObject` (memory-mapped file — veri),
  - `Global\WCHSFDRequestObject`, `Global\WCHSFDResponseObject` (istek/yanıt event'leri).
- **Ayarların saklanması:** Registry `Software\WinChipHead\ufdd`
  - `LastFormatType`, `LastFddFormatCout`, `QuickFormat`, `EnableBootDisk`, `BootDiskDOSVer`, `BackupPath`.
- **USB üzerindeki disk düzeni:** blok dizin kalıbı `%s\%c\FLOPPY%03d` → örn. `DISK\F\FLOPPY000`
  - `%c` = bloğun bağlandığı sürücü harfi, `%03d` = blok numarası (000..099).

### 2.3 Çıkarılan özellik listesi (ikiliden)

**Format / bölümleme**
- `Format USB Flash Drive` (Ctrl+F) — tüm USB'yi siler, N blok oluşturur. Diyalog: `format_dlg`.
- Blok formatı — diyalog: `format_fdd_dlg`, `Select floppy disk format`.
  - Tekli: `&Format...` / *Format current floppy disk block*
  - Hızlı: `&Quick format` / `QuickFormat`
  - Toplu: `Bulk &format...` / *Format several floppy disk blocks which are selected*
- Kapasite tipleri: **720K**, **1.2M**, **1.44M**
- Dosya sistemi: **yalnızca FAT12** (`Error: Not FAT12 file system`, imaj `< 1.5M` olmalı)
- Bootable DOS disketi: `EnableBootDisk`, `BootDiskDOSVer`, **DOS 6.22 / DOS 7.1**, `MSDOS.SYS` yazımı
- Blok sayısı aralığı: **1–100** (`the number of floppy disk block (1 - 100)`)
- Uyarılar: `Alarm: Format will delete all the data!!!`, `Ensure to format?`

**Görünüm / listeleme**
- Görünümler: Büyük simge / Küçük simge / Liste / **Detay**
- Detay sütunları: `Serial number | Capability | Used | Renew time`
- `Enumerate floppy disk block: %d`, Refresh / Reread, cihaz seri no gösterimi
- Kapasite biçimi: `%0.1f KB` / `%0.2f MB`
- Tekli/çoklu seçim, "tümünü seç"

**Veri işlemleri**
- İmaj yaz / blok→imaj kaydet (`*.img`), toplu write/save
- Upload/Download blok (PC klasörü ↔ blok)
- Tüm USB yedekleme (Ctrl+B → `\DISKBAK`, `BackupPath`)
- Opsiyonlar: aç sonrası Explorer açma, upload klasörünü otomatik temizleme, otomatik yeniden adlandırma, orijinalin üzerine yazma

**Menü/kısayol haritası**
`File`, `USB Flash Drive`, `Option`, `Help` — Ctrl+F (USB format), Ctrl+B (yedek), Ctrl+D (upload dizini), Ctrl+Q (ayarlar), Ctrl+A (tümünü seç), F1 (yardım).

---

## 3. Veri Modeli

### 3.1 "Floppy disk block" (blok / slot) kavramı

USB bellek, mantıksal olarak **sabit boyutlu bloklara** bölünür. Her blok = tam bir disket imajı (ham sektör dizisi). Önerilen model:

```
USB BELLEK
├─ Yönetim/başlık alanı (opsiyonel: blok tablosu, meta veri)
├─ BLOCK 0  → tam disket imajı (ör. 1.44MB = 2880 × 512B)
├─ BLOCK 1  → tam disket imajı
├─ ...
└─ BLOCK N-1
```

Her blok için saklanması önerilen meta veri:

| Alan | Açıklama |
|---|---|
| index | Blok numarası (0..N-1) |
| geometry | 720K / 1.2M / 1.44M (sektör/iz, iz sayısı, kafa) |
| serial | Birim seri numarası (volume serial) |
| capacity | Toplam kapasite (byte) |
| used | Kullanılan alan (FAT'ten hesaplanır) |
| label | Birim etiketi (volume label) |
| mtime | Son değişiklik ("Renew time") |
| bootable | DOS bootable mı |

> **Donanım DOĞRULANDI (2026-11-16): Alfais 4408 = SFR1M44-U100 ailesi.**
> Bu, sizdeki ipcas/WCH "USB Floppy Manager v1.40i" yazılımının yönettiği endüstriyel emülatördür (düz dosya okuyan Gotek/FlashFloppy **değil**). Kesin ham USB yerleşimi:
>
> - USB, art arda **1.5 MiB (0x180000 bayt) "banka"lara** bölünür; partition tablosu yoktur.
> - Banka N ofseti = **N × 0x180000**.
> - Her bankanın ilk **1.474.560 baytı** = standart 1.44MB FAT12 disket imajı; kalan ~96 KB sıfır dolgu.
> - U100 = 100 banka (ilk 150 MiB kullanılır), U100K = 1000 banka (1500 MiB).
> - Windows yalnızca banka 000'ı (ofset 0) sürücü olarak görür; diğerleri ham ofsetlerde.
> - Aktif banka cihazın ön panelindeki 3 haneli ekran/düğmelerle seçilir.
>
> **Sonuç:** Dosya-tabanlı `BlockSet` modeli bu cihaz için GEÇERSİZ. Doğru model `crates/floppyctl/src/sfr.rs` içinde: tüm USB'nin ham imajını (bankalar doğru ofsetlerde) üretip Rufus/Win32DiskImager/dd ile USB'ye yazmak. (Kaynak: cihaz tersine mühendislik incelemesi — goughlui.com SFR1M44-U100K review.)

### 3.2 Erişim yolu seçenekleri

1. **Ham sektör erişimi** (referans uygulamanın yaptığı): USB'ye `\\.\PhysicalDriveN` üzerinden ham okuma/yazma. Her bloğun ofseti = `index × blok_boyutu`. Admin + kernel servisi gerektirir.
2. **Dosya tabanlı** (ör. Gotek/FlashFloppy): her disket bir `.img`/`.hfe` **dosyası** olarak FAT32 USB üzerinde durur. Çok daha basit, admin gerektirmez. (Modern yaklaşım — önerilir.)

Hangisinin uygun olduğu hedef emülatör firmware'ine bağlıdır.

---

## 4. Floppy Format Teknik Detayları (FAT12)

Bir formatlayıcı yazmak için gereken tüm geometri ve yapı burada.

### 4.1 Disket geometrileri

| Format | Boyut | Bytes/sek | Sek/iz | Kafa | İz | Toplam sek | Medya | Sek/FAT | Root giriş | Sek/küme |
|---|---|---|---|---|---|---|---|---|---|---|
| 720K (3.5" DD) | 737.280 | 512 | 9 | 2 | 80 | 1440 | 0xF9 | 3 | 112 | 2 |
| 1.2M (5.25" HD) | 1.228.800 | 512 | 15 | 2 | 80 | 2400 | 0xF9 | 7 | 224 | 1 |
| 1.44M (3.5" HD) | 1.474.560 | 512 | 18 | 2 | 80 | 2880 | 0xF0 | 9 | 224 | 1 |

(Tamlık için: 2.88M → sek/iz 36, toplam 5760, medya 0xF0, sek/FAT 9, cluster 2. Referans uygulamada görünmüyor.)

### 4.2 Disket mantıksal yerleşimi (1.44MB örneği)

```
Sektör 0            : Boot sektörü (BPB içerir)
Sektör 1..9         : FAT #1   (9 sektör)
Sektör 10..18       : FAT #2   (9 sektör, kopya)
Sektör 19..32       : Root dizin (224 giriş × 32B = 7168B = 14 sektör)
Sektör 33..2879     : Veri alanı (cluster 2'den başlar)
```

### 4.3 Boot sektörü / BPB alanları (ofsetler)

| Ofset | Boyut | Alan | 1.44M değeri |
|---|---|---|---|
| 0x00 | 3 | Jump | `EB 3C 90` |
| 0x03 | 8 | OEM adı | ör. `"MSDOS5.0"` |
| 0x0B | 2 | Bytes/sektör | 512 |
| 0x0D | 1 | Sektör/küme | 1 |
| 0x0E | 2 | Reserved sektör | 1 |
| 0x10 | 1 | FAT sayısı | 2 |
| 0x11 | 2 | Root giriş | 224 |
| 0x13 | 2 | Toplam sektör (16-bit) | 2880 |
| 0x15 | 1 | Medya tanımlayıcı | 0xF0 |
| 0x16 | 2 | Sektör/FAT | 9 |
| 0x18 | 2 | Sektör/iz | 18 |
| 0x1A | 2 | Kafa sayısı | 2 |
| 0x1C | 4 | Gizli sektör | 0 |
| 0x20 | 4 | Toplam sektör (32-bit) | 0 |
| 0x24 | 1 | Sürücü no | 0x00 |
| 0x26 | 1 | Boot imzası | 0x29 |
| 0x27 | 4 | Birim seri no | rastgele |
| 0x2B | 11 | Birim etiketi | `"NO NAME    "` |
| 0x36 | 8 | FS tipi | `"FAT12   "` |
| 0x1FE | 2 | İmza | `0x55 0xAA` |

### 4.4 FAT12 başlatma

- İlk iki FAT girişi rezerve: FAT[0] = medya baytı + 0xF00 (ör. `F0 FF FF`), FAT[1] = EOC.
- FAT12'de her giriş 12 bit → 2 giriş = 3 byte paketleme.
- Boş disk için FAT'in geri kalanı 0x00; root dizin ve veri alanı 0x00.

### 4.5 Quick format vs tam format

- **Quick format:** Yalnızca boot sektörü + FAT'ler + root dizin sıfırlanıp yazılır (veri alanına dokunulmaz). Hızlı.
- **Tam format:** Tüm sektörler 0x00 (veya 0xF6) ile yazılıp ardından yapı kurulur. (Opsiyonel "verify".)

### 4.6 Bootable DOS disketi

- Boot sektörüne gerçek boot kodu yazılır (sadece BPB değil).
- Root dizine sistem dosyaları: `IO.SYS`, `MSDOS.SYS` (DOS 6.22) veya tek `IO.SYS`/`COMMAND.COM` düzeni; FreeDOS için `KERNEL.SYS` + `COMMAND.COM`.
- DOS 6.22 ve DOS 7.1 (Win9x) boot kodu/BPB ayrıntıları farklı → sürüm seçimine göre şablon.
- **Yasal uyarı:** MS-DOS boot dosyaları telifli; dağıtılabilir bir uygulama için **FreeDOS** kullanılması önerilir.

---

## 5. Partition / Blok Gösterme

Önerilen liste görünümü sütunları (referansla uyumlu):

```
# | Seri No | Kapasite | Kullanılan | % Dolu | Etiket | Format | Son Değişiklik | Boot
```

- Kapasite/kullanım FAT taranarak hesaplanır: boş küme sayısı × küme boyutu.
- Görünüm modları: Büyük simge / Küçük simge / Liste / Detay.
- Yenileme: aygıt yeniden sayımı + her bloğun BPB + FAT özetini okuma.
- Boş/dolu/bozuk blokları renk/ikon ile ayırt etme (UX iyileştirmesi).

---

## 6. Fonksiyonel Gereksinimler (özet liste)

**Aygıt**
- [ ] USB disket emülatör aygıtını algıla ve seç
- [ ] Blokları say/listele (1–100), meta veriyi oku
- [ ] 4 görünüm modu + yenileme

**Format**
- [ ] USB'yi N bloğa böl (toplam format)
- [ ] Tek blok format (720K / 1.2M / 1.44M, FAT12)
- [ ] Quick format
- [ ] Toplu (çoklu seçim) format
- [ ] Bootable DOS disketi oluşturma (FreeDOS önerilir)
- [ ] Yıkıcı işlemlerde onay diyaloğu
- [ ] Son format ayarını hatırlama

**Veri**
- [ ] Bloğa dosya yükle / bloktan dosya indir
- [ ] Bloğa `.img` yaz / bloğu `.img` olarak kaydet
- [ ] Toplu yaz/kaydet
- [ ] Tüm USB'yi yedekle / geri yükle

**Sistem**
- [ ] Admin yükseltme (ham erişim gerekiyorsa)
- [ ] Ayar kalıcılığı (registry veya config dosyası)

---

## 7. Teknik Kısıtlar ve Riskler

| Konu | Not |
|---|---|
| Ham disk erişimi | `\\.\PhysicalDriveN` yazma admin ister; yanlış sürücüye yazma **veri kaybı** riski → aygıt doğrulaması şart |
| Donanım protokolü | Blok tablosu/aktif blok seçimi firmware'e özgü — tersine mühendislik veya üretici dokümanı gerekir (doğrulanması gereken) |
| FAT12 sınırı | İmaj < 1.5 MB; yalnızca FAT12 |
| DOS boot telifi | MS-DOS yerine FreeDOS |
| İşletim sistemi | Windows'ta kernel servisi; modern eşdeğeri Win10/11 sürücü imzalama gerektirir |
| Eşzamanlılık | Aygıt meşgulken (hedef makine okurken) yazma çakışması |

---

## 8. Geliştirme Önerileri / Yol Haritası

**Teknoloji kararı (seçildi): Rust + Tauri**
- **Çekirdek (`fat12` crate):** Byte-doğruluğu kritik FAT12/disk işlemleri Rust'ta. `#[repr(C, packed)]` struct'larla BPB birebir modellenir; `fatfs` crate'i ile bağımsız doğrulama (round-trip test).
- **GUI:** Tauri (tek, küçük native binary; Windows/Linux/macOS). Çekirdek mantık GUI'den ayrık — CLI olarak da kullanılabilir.
- **Ham cihaz erişimi:** `\\.\PhysicalDrive` (Win) / `/dev/sd*` (Linux) / `/dev/disk*` (macOS), `#[cfg(target_os)]` ile izole.
- **Ortam (bu makinede doğrulandı):** rustc/cargo 1.99.1, Node 26.12 (Tauri için).

**Önerilen sıra**
1. Hedef donanımın erişim modelini belirle (dosya-tabanlı mı, ham blok mu). *Her şeyin temeli bu.*
2. Bağımsız bir **FAT12 imaj kütüphanesi** yaz (oluştur/format/oku/yaz, boot sektörü + FAT + root). Birim testlerle; Bölüm 4 tablolarını referans al.
3. İmajı gerçek bir disket sürücüsünde/emülatörde doğrula (round-trip: oluştur → hedef makinede oku).
4. Blok yönetimi (listeleme + meta veri) katmanı.
5. GUI (liste/detay görünüm, format diyalogları).
6. Veri aktarımı (upload/download, imaj import/export), yedekleme.
7. Bootable disk + toplu işlemler.

**İlk doğrulama testi (minimum)**
Boş bir 1.44MB FAT12 imajı üret → Windows'ta döngü aygıtı/sanal disket olarak bağla → biçim ve dosya yazma/okumayı teyit et. Bu, formatlayıcının çekirdeğini kanıtlar.

---

## 9. Ekler

- Çıkarılan ham string dosyaları (bu oturumun scratchpad'inde): `exe_strings.txt` (ASCII), `exe_unicode.txt` (UTF-16).
- Referans kılavuzlar: `FDEv2_20120711en.pdf`, `FDEv2 Readme.pdf` (uygulama kurulum dizininde).
