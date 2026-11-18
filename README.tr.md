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
- Blok başına hızlı format ve disket etiketi düzenleme; bir blok **aralığını tek seferde formatla**.
- Bir bloğu FreeDOS ile **bootable yap** (kernel + `COMMAND.COM`).
- Her yazma **geri okunup doğrulanır** (tüm USB yazımlarında isteğe bağlı), canlı ilerleme
  çubuğuyla.

### Bloklar
- **Bloğu `.img` olarak kaydet** / **bloğa `.img` yükle** (1.44 MB imajlar veya dolgulu
  1.5 MiB banka).
- Bir bloğu başka bir bloğa **kopyala**.
- USB'deki tüm bloklarda dosya ve klasör adlarında **ara**.
- **Birden çok blok seç** (Ctrl/Shift‑tık); tek seferde formatla, `.img` olarak kaydet veya
  kontrol et.
- **Çok sayıda `.img` dosyasını** ardışık bloklara bir kerede yükle (doğal ad sırasıyla
  yazılır; hiçbir şey yazılmadan önce tüm dosyalar kontrol edilir).
- **Bütünlük kontrolü**: her bloğu sağlam / uyarı / bozuk / boş olarak işaretler (boot
  sektörü, FAT tutarlılığı, her dosyanın geri okunması); sorunlar ipucunda görünür.
- **Liste veya ızgara görünümü**; ızgara her disketin doluluğunu gösterir.

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
- USB'yi (veya ilk N bankasını) ham bir `.img` dosyasına **yedekle**, sonra geri yükle.

### Kolaylıklar
- USB bellek takıldığında / çıkarıldığında **otomatik algılanır**.
- Son cihazı, format ayarlarını ve sekmeyi hatırlar. İngilizce / Türkçe arayüz.
- Yeni sürüm yayımlandığında haber verir (GitHub Releases'ı günde en fazla bir kez kontrol
  eder; Ayarlar'dan kapatılabilir).

### CLI (`floppyctl`)
```sh
floppyctl format   --size 1.44 --label MYDISK --out disk.img [--add FILE]
floppyctl info     disk.img
floppyctl build-usb --out usb.img --banks 100 --bank 0:1.44:BOOTDISK --add-to 0:readme.txt
floppyctl list-usb
floppyctl read-bank --drive 3 --index 0        # veya --image usb.img --index 0
floppyctl write-usb --image usb.img --index <IDX> --yes   # yıkıcı

# Blok komutları USB bellekte (--drive N) veya ham USB imaj dosyasında (--image usb.img) çalışır
floppyctl check        --drive 3 --count 100            # bozuk blok varsa çıkış kodu 2
floppyctl search       --drive 3 rapor
floppyctl export-banks --drive 3 --from 0 --count 10 --out-dir yedek/
floppyctl import-banks --drive 3 --start 20 --yes yedek/*.img
floppyctl clone-bank   --drive 3 --from 0 --to 5 --yes
floppyctl format-banks --drive 3 --from 10 --count 5 --label WORK --yes [--freedos]
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

## İndirme

Kurulum dosyaları [Releases](https://github.com/akbulut97/floppy-workbench/releases)
sayfasında yayınlanır: Windows (`.exe` kurulum / `.msi`), Linux (`.AppImage` / `.deb`) ve
macOS (`.dmg`). **Kod imzalı değildir**; bu yüzden Windows SmartScreen ve macOS Gatekeeper ilk
açılışta uyarı verir (macOS: sağ tık → Aç).

## Gereksinimler

- **Windows 10/11**, **Linux** veya **macOS**.
- USB cihazına ham erişim:
  - Windows: **Yönetici** (release derlemesi yükseltmeyi otomatik ister).
  - Linux: **root** olarak çalıştırın (ör. `sudo floppy-workbench`) ya da kullanıcınızı `disk`
    grubuna ekleyin. Yazmadan önce USB'nin bağlı bölümleri ayrılır (unmount).
  - macOS: `sudo` ile çalıştırın; yazmadan önce bellek `diskutil` ile ayrılır.
- Windows'ta **WebView2** (Windows 11'de hazır gelir); Linux'ta WebKitGTK 4.1.
- Derlemek için: güncel bir **Rust** zinciri ve **Node** (yalnızca Tauri CLI önkoşulları;
  arayüzün kendisi bağımlılıksız, düz HTML). Linux'ta ayrıca
  `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.

> Gerçek donanım testleri Windows'ta yapıldı. Linux ve macOS cihaz yolları CI'da derlenip
> birim testlerinden geçiyor ama daha az denendi — önce yedek bir USB ile deneyin.

## Derleme ve çalıştırma

```sh
# çekirdek testleri
cargo test

# CLI
cargo run -p floppyctl -- info disk.img

# GUI (cihaz erişimi için derlenen exe'yi Yönetici olarak çalıştır)
cargo build --release -p floppy-gui
# → target/release/floppy-gui.exe   (tek taşınabilir dosya, ~6 MB)

# kurulum dosyaları (Windows'ta NSIS/MSI, Linux'ta AppImage/deb, macOS'ta dmg)
cd crates/floppy-gui && npx --yes @tauri-apps/cli@2.12.6 build
# → target/release/bundle/
```

Bir `v*` etiketi push edildiğinde release iş akışı üç platform için kurulum dosyalarını
derler ve bunları taslak (draft) bir GitHub sürümüne ekler. CI; Windows, Linux ve macOS'ta
`cargo fmt --check`, `clippy -D warnings` ve testleri çalıştırır.

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
doğrulanır. Tüm akış (format, yazma, geri okuma) Windows'ta gerçek bir SFR1M44‑U100
emülatörüyle test edildi ve her yazma geri okunarak doğrulanır. Linux ve macOS desteği daha
yeni — güvenmeden önce kendi emülatörünüz ve yedek bir USB ile teyit edin.

## Lisans

MIT.

## Teşekkür

Cihaz yerleşimi, SFR1M44‑U100K üzerine topluluk tersine mühendislik çalışmalarından
yararlanılarak çözüldü; FAT12 çıktısı `fatfs` crate'iyle çapraz doğrulanır. WCH, ipcas, Gotek
veya Alfais ile bağlantılı değildir.
