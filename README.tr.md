# Floppy Workbench

[English](README.md) · **Türkçe**

**SFR1M44‑U100 / Alfais 4408** ailesi **USB disket emülatörleri** için çapraz‑platform bir
masaüstü uygulaması ve CLI. Sıradan bir USB belleği, bu cihazların beklediği ham yerleşime
formatlar; saklanan disketleri bir dosya‑gezgini arayüzüyle gözatıp düzenlemenizi sağlar.

> Bu endüstriyel emülatörler makinenin 34 pinli floppy konektörüne takılır ve en fazla 100
> sabit boyutlu **floppy "banka"sına** bölünmüş bir USB belleği okur; her banka tam bir
> 1.44 MB FAT12 disket imajıdır. Floppy Workbench bu yerleşimi oluşturur ve yönetir.

**Rust** (çekirdek + CLI) ve **Tauri 2** (GUI) ile yazılmıştır. FAT12 formatlayıcı, bağımsız
[`fatfs`](https://crates.io/crates/fatfs) crate'iyle doğrulanır.

---

## Özellikler

### Format
- USB'yi **N** adet eşit boyutlu floppy bankasına böl (1.44 MB / 1.2 MB / 720 KB, FAT12).
- Doğrudan cihaza yaz ya da Rufus / `dd` ile yazmak için ham bir `usb.img` üret.
- Blok başına hızlı format ve disket etiketi düzenleme.

### İçerik (Contents) — gerçek bir dosya gezgini
- İki panel: solda bloklar, sağda seçili disketin dosyaları.
- Breadcrumb ile **klasör gezinme**; gir / yukarı çık.
- **Dosya ekle** (sürükle‑bırak veya yerel dosya penceresi), **Yeni klasör**.
- Klasörler ve bloklar arası **Kes / Kopyala / Yapıştır** (klasörler için özyinelemeli).
- **Yeniden adlandır**, **Sil** (özyinelemeli), PC'ye **Dışa aktar** (yerel "Farklı Kaydet").
- Metin ve görsel dosyaları uygulama içinde **önizle** (çift tık).
- Seçili öğe için **ayrıntılar paneli**.

### Gelişmiş (Advanced)
- Çok bankalı USB imajı üret, mevcut imajı yaz veya tek bir floppy imajı formatla.

### CLI (`floppyctl`)
```sh
floppyctl format   --size 1.44 --label MYDISK --out disk.img [--add FILE]
floppyctl info     disk.img
floppyctl build-usb --out usb.img --banks 100 --bank 0:1.44:BOOTDISK --add-to 0:readme.txt
floppyctl list-usb
floppyctl read-bank --drive 3 --index 0        # veya --image usb.img --index 0
floppyctl write-usb --image usb.img --index <IDX> --yes   # yıkıcı
```

---

## Cihaz yerleşimi (SFR1M44‑U100)

Bu araç tarafından tersine mühendislikle çözülüp yeniden üretilir:

- USB, art arda dizilen **1.5 MiB banka** dizisidir; banka *N* `N × 0x180000` ofsetinde başlar.
- Her bankanın ilk **1.474.560 baytı** standart bir 1.44 MB FAT12 disketidir; kalanı sıfır dolgu.
- **Partition tablosu yoktur** — cihaz bankayı indeksle seçer (ön panel ekranı).
- U100 = 100 banka; U100K = 1000 banka. Windows yalnızca banka 000'ı (ofset 0) bağlar.

Tam teknik rapor için: [`TEKNIK_RAPOR.md`](TEKNIK_RAPOR.md) ([İngilizce](TECHNICAL_REPORT.md)).

---

## Gereksinimler

- **Windows** (GUI ve doğrudan cihaz G/Ç şimdilik yalnızca Windows).
- Ham USB cihazını okumak/yazmak için **Yönetici** hakları.
- **WebView2** çalışma zamanı (Windows 11'de hazır gelir).
- Derlemek için: güncel bir **Rust** zinciri ve **Node** (yalnızca Tauri CLI önkoşulları;
  arayüzün kendisi bağımlılıksız, düz HTML).

## Derleme ve çalıştırma

```sh
# çekirdek testleri
cargo test

# CLI
cargo run -p floppyctl -- info disk.img

# GUI (cihaz erişimi için derlenen exe'yi Yönetici olarak çalıştır)
cargo build --release -p floppy-gui
# → target/release/floppy-gui.exe   (tek taşınabilir dosya, ~6 MB)
```

---

## Proje yapısı

```
crates/
├─ fat12/      FAT12 imaj formatlayıcı (geometri, boot/FAT/kök dizin, bootable, etiket, dosya)
├─ floppyctl/  CLI + çekirdek: SFR1M44 banka yerleşimi, ham cihaz G/Ç, path‑temelli fs işlemleri
└─ floppy-gui/ Tauri 2 masaüstü uygulaması (çerçevesiz, npm'siz düz arayüz)
```

## Güvenlik

Fiziksel bir diske yazmak **yıkıcı ve geri alınamaz**. Floppy Workbench yalnızca
**removable (çıkarılabilir)** sürücüleri listeler, her yazmadan önce hedef cihaz numarasını
yeniden doğrular ve onay ister — sistem diski kazara hedeflenemez. Yine de **önce yedek bir
USB ile deneyin** ve seçili sürücüyü iki kez kontrol edin.

## Durum

FAT12 çekirdeği ve cihaz yerleşimi testlerle kaplıdır ve bağımsız bir FAT uygulamasıyla
doğrulanır. Çıkarılabilir sürücü algılama gerçek donanımla doğrulandı; ancak
**ham yazma/okuma yolu henüz fiziksel bir cihazda uçtan uca doğrulanmadı** — kendi
emülatörünüzle teyit edene kadar yıkıcı işlemleri deneysel kabul edin.

## Lisans

MIT.

## Teşekkür

Cihaz yerleşimi, SFR1M44‑U100K üzerine topluluk tersine mühendislik çalışmalarından
yararlanılarak çözüldü; FAT12 çıktısı `fatfs` crate'iyle çapraz doğrulanır. WCH, ipcas, Gotek
veya Alfais ile bağlantılı değildir.
