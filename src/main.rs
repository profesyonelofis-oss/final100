#![windows_subsystem = "windows"]

use chrono::{Datelike, Timelike};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

mod iller;
mod musteriler;

/// İşletmenin web varlığı. Site yaptırmaya niyet göstergesidir: sosyal medya
/// hesabı olup sitesi olmayan işletme, çevrimiçi görünmek istediğini zaten
/// göstermiş ama henüz site yaptırmamış demektir — en sıcak hedef kitle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
enum SiteDurumu {
    /// Hiçbir web etiketi yok.
    Yok,
    /// Yalnızca Instagram/Facebook/Linktree gibi bir sosyal medya adresi var.
    Sosyal,
    /// Google business.site, WixSite, Blogspot gibi ücretsiz bir araçla kurulmuş site.
    Ucretsiz,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct BusinessLead {
    name: String,
    address: String,
    category: String,
    phone: String,
    selected: bool,
    kaynak: String,
    konum: Option<(f64, f64)>,
    calisma_saatleri: String,
    demo_yolu: Option<String>,
    mesaj_hazir: bool,
    /// Kayıttaki ek bilgi miktarı (0-10): mutfak türü, marka, engelli erişimi
    /// gibi puanlanmayan etiketler. İşletmenin gerçekten var olan ve birileri
    /// tarafından takip edilen bir yer olduğunun işareti olarak kullanılır.
    kayit_dolulugu: u8,
    /// Web varlığı: site yok / yalnızca sosyal medya / ücretsiz site aracı.
    site: SiteDurumu,
    /// Bulunan sosyal medya veya ücretsiz site adresi.
    site_linki: String,
    /// OSM kaydının son düzenlenmesinden bu yana geçen gün (tazelik).
    son_guncelleme_gun: Option<i64>,
    /// Google'daki inceleme sayısı: gerçek müşteri trafiği ve bütçe göstergesi.
    inceleme_sayisi: Option<u32>,
    /// Google'daki ortalama yıldız puanı (0-5). Yalnızca Google kaynağında gelir;
    /// OpenStreetMap'te yorum/puan verisi yoktur. Karnede "4,2 ★ · 37 yorum"
    /// olarak gösterilir.
    #[serde(default)]
    yildiz_puani: Option<f32>,
    /// Kalıcı defterdeki iletişim durumu (yeni / gönderildi / kara liste...).
    durum: musteriler::Durum,
    /// Demo GitHub Pages'e yayınlandıysa paylaşılabilir adresi.
    #[serde(default)]
    yayin_linki: Option<String>,
    /// Aynı yayındaki dijital karnenin adresi (karne üretilemediyse boş).
    #[serde(default)]
    karne_linki: Option<String>,
    /// Defterden gelen mesaj varyantı (A/B denemesi). Gönderimde bu kullanılır.
    #[serde(default)]
    mesaj_varyanti: Option<musteriler::Varyant>,
}

/// Taramada kullanılacak veri kaynağı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kaynak {
    /// OpenStreetMap/Overpass: ücretsiz, verisi saklanabilir, ama Türkiye'de
    /// telefon/çalışma saati kapsaması zayıf.
    Osm,
    /// Google Places Text Search: telefon, çalışma saati ve inceleme sayısı
    /// gelir; aylık ücretsiz kota vardır.
    Google,
    /// OSM birincil, OSM boş dönen ilçede Google yedeği (varsayılan).
    Hibrit,
}

impl Kaynak {
    /// Arayüzdeki seçim sırası.
    const HEPSI: [Kaynak; 3] = [Kaynak::Hibrit, Kaynak::Google, Kaynak::Osm];

    fn etiket(self) -> &'static str {
        match self {
            Kaynak::Osm => "OSM",
            Kaynak::Google => "Google",
            Kaynak::Hibrit => "Hibrit",
        }
    }

    /// Günlükte gösterilecek uzun açıklama.
    fn aciklama(self) -> &'static str {
        match self {
            Kaynak::Osm => "OpenStreetMap (ücretsiz, iletişim bilgisi seyrek)",
            Kaynak::Google => "Google Places (telefon + inceleme sayısı, kotalı)",
            Kaynak::Hibrit => "OSM + boş ilçede Google yedeği",
        }
    }
}

/// Sonuç listesinin süzgeci. "Takip" günlük çalışma listesidir: mesaj
/// gönderilmiş, üzerinden `TAKIP_GUN` gün geçmiş ve hâlâ yanıt bekleyenler.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListeSuzgeci {
    Hepsi,
    Yeni,
    Takip,
}

/// Mesaj gönderildikten kaç gün sonra takip listesine düşer.
const TAKIP_GUN: i64 = 3;

/// Bir günde hazırlanabilecek en fazla WhatsApp mesajı.
///
/// Mesajlar elle gönderilse de aynı numaradan arka arkaya 20-30 sohbet
/// açmak hesabı spam olarak işaretletir. Fren defterde tutulur: uygulama
/// kapatılıp açılsa da günlük sayaç sıfırlanmaz.
const GUNLUK_GONDERIM_LIMITI: usize = 15;

/// İki `YYYY-MM-DD` tarihi arasındaki gün farkı.
fn gun_farki(eski: &str, yeni: &str) -> Option<i64> {
    let once = chrono::NaiveDate::parse_from_str(eski, "%Y-%m-%d").ok()?;
    let sonra = chrono::NaiveDate::parse_from_str(yeni, "%Y-%m-%d").ok()?;
    Some((sonra - once).num_days())
}

/// Arka plandaki tarama/gönderim işinin arayüze gönderdiği mesajlar.
enum Mesaj {
    Log(String),
    /// İl geneli taramasında tamamlanan ilçe sayısı ve toplam ilçe.
    Ilerleme {
        tamamlanan: usize,
        toplam: usize,
    },
    TaramaBitti(Vec<BusinessLead>),
    TaramaBasarisiz,
}

pub struct LeadApp {
    cities: Vec<(&'static str, Vec<&'static str>)>,
    sectors: Vec<&'static str>,

    selected_city_idx: usize,
    selected_district_idx: usize,
    selected_sector_idx: usize,

    city_search: String,
    district_search: String,
    sector_search: String,
    sonuc_arama: String,

    leads: Vec<BusinessLead>,
    /// Seçili arama kaynağı (OSM / Google / Hibrit).
    kaynak: Kaynak,
    /// Sonuç listesinin süzgeci (tümü / yalnızca yeni / takip bekleyenler).
    liste_suzgeci: ListeSuzgeci,
    /// Kalıcı müşteri defteri: durumlar, kara liste ve mükerrer engeli.
    defter: musteriler::Defter,

    whatsapp_phone: String,
    whatsapp_status: String,
    is_whatsapp_connected: bool,
    whatsapp_acik: bool,
    detay_idx: Option<usize>,
    /// Gönderim öncesi önizleme: (liste indeksi, hazır mesaj metni).
    onizleme: Option<(usize, String)>,
    /// Takip kuyruğu paneli (yanıt bekleyen, zamanı gelmiş işletmeler).
    takip_panel_acik: bool,
    ilce_kaydir: bool,
    /// Sektör seçim paneli açık mı ve sol sütunda hangi grup seçili.
    sektor_panel_acik: bool,
    sektor_panel_grup: usize,
    /// Açıkken tarama tek ilçe yerine ilin tüm ilçelerini sırayla gezer.
    il_geneli: bool,
    /// Iki taraması boş döndüyse uyarı gösterilir; taranan ilçenin adını tutar.
    bos_ilce_uyari: Option<String>,
    /// İl geneli taramasında (tamamlanan, toplam) ilçe sayısı.
    ilerleme: Option<(usize, usize)>,

    logs: Vec<String>,
    is_running: bool,
    log_rx: Receiver<Mesaj>,
    log_tx: Sender<Mesaj>,
}

impl Default for LeadApp {
    fn default() -> Self {
        let (tx, rx) = channel();

        let cities: Vec<(&'static str, Vec<&'static str>)> = iller::ILLER
            .iter()
            .map(|(il, ilceler)| (*il, ilceler.to_vec()))
            .collect();
        let varsayilan_ilce = cities[0]
            .1
            .iter()
            .position(|ilce| *ilce == "Beşiktaş")
            .unwrap_or(0);

        let sectors: Vec<&'static str> = SEKTORLER.iter().map(|sektor| sektor.ad).collect();

        Self {
            cities,
            sectors,
            kaynak: Kaynak::Hibrit,
            liste_suzgeci: ListeSuzgeci::Hepsi,
            defter: musteriler::Defter::yukle(),
            selected_city_idx: 0, // İstanbul (listenin ilk satırı)
            selected_district_idx: varsayilan_ilce,
            selected_sector_idx: 0,

            city_search: "".to_string(),
            district_search: "".to_string(),
            sector_search: "".to_string(),
            sonuc_arama: "".to_string(),

            leads: vec![],

            whatsapp_phone: "+90 500 000 00 00".to_string(),
            whatsapp_status: "Numara doğrulanmadı".to_string(),
            is_whatsapp_connected: false,
            whatsapp_acik: false,
            detay_idx: None,
            onizleme: None,
            takip_panel_acik: false,
            ilce_kaydir: true,
            sektor_panel_acik: false,
            sektor_panel_grup: 0,
            il_geneli: false,
            bos_ilce_uyari: None,
            ilerleme: None,

            logs: vec!["[Sistem] Arama çubuklu veritabanı yüklendi.".to_string()],
            is_running: false,
            log_rx: rx,
            log_tx: tx,
        }
    }
}

// ---------------------------------------------------------------------------
// Tasarım sistemi
//
// Koyu, katmanlı yüzeyler + tek vurgu rengi (amber). Tüm ölçüler burada
// tanımlıdır; arayüzün geri kalanı bu sabitleri kullanır.
// ---------------------------------------------------------------------------

const ARKA_PLAN: egui::Color32 = egui::Color32::from_rgb(10, 12, 17);
const PANEL: egui::Color32 = egui::Color32::from_rgb(14, 17, 23);
const KART: egui::Color32 = egui::Color32::from_rgb(19, 23, 31);
const KART_SECILI: egui::Color32 = egui::Color32::from_rgb(25, 30, 40);
const GIRINTI: egui::Color32 = egui::Color32::from_rgb(11, 14, 19);
const KENAR: egui::Color32 = egui::Color32::from_rgb(32, 38, 49);
const KENAR_GUCLU: egui::Color32 = egui::Color32::from_rgb(58, 67, 84);
const METIN: egui::Color32 = egui::Color32::from_rgb(234, 238, 245);
const SOLUK: egui::Color32 = egui::Color32::from_rgb(141, 151, 169);
const DIM: egui::Color32 = egui::Color32::from_rgb(97, 107, 124);
const VURGU: egui::Color32 = egui::Color32::from_rgb(246, 168, 39);
const VURGU_ACIK: egui::Color32 = egui::Color32::from_rgb(255, 201, 96);
const YESIL: egui::Color32 = egui::Color32::from_rgb(62, 214, 152);
const KIRMIZI: egui::Color32 = egui::Color32::from_rgb(255, 124, 124);
const MAVI: egui::Color32 = egui::Color32::from_rgb(126, 178, 255);
const KOYU_YAZI: egui::Color32 = egui::Color32::from_rgb(22, 16, 5);

const YUVARLAK_KART: f32 = 14.0;
const YUVARLAK_KUCUK: f32 = 9.0;

fn karistir(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let kanal = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        kanal(a.r(), b.r()),
        kanal(a.g(), b.g()),
        kanal(a.b(), b.b()),
        kanal(a.a(), b.a()),
    )
}

fn sektor_rengi(sektor: &str) -> egui::Color32 {
    if sektor.contains("Restoran") || sektor.contains("Kafe") {
        egui::Color32::from_rgb(255, 172, 92)
    } else if sektor.contains("Otel") {
        egui::Color32::from_rgb(126, 186, 255)
    } else if sektor.contains("Diş") || sektor.contains("Sağlık") {
        egui::Color32::from_rgb(94, 226, 191)
    } else if sektor.contains("Kuaför") || sektor.contains("Güzellik") {
        egui::Color32::from_rgb(246, 149, 200)
    } else if sektor.contains("Oto") {
        egui::Color32::from_rgb(168, 182, 206)
    } else if sektor.contains("Yazılım") || sektor.contains("Bilişim") {
        egui::Color32::from_rgb(167, 139, 250)
    } else if sektor.contains("Spor") {
        egui::Color32::from_rgb(255, 138, 128)
    } else {
        egui::Color32::from_rgb(180, 196, 255)
    }
}

fn govde_font(boyut: f32) -> egui::FontId {
    egui::FontId::new(boyut, egui::FontFamily::Proportional)
}

fn baslik_font(boyut: f32) -> egui::FontId {
    egui::FontId::new(boyut, egui::FontFamily::Name("Baslik".into()))
}

fn kod_font(boyut: f32) -> egui::FontId {
    egui::FontId::new(boyut, egui::FontFamily::Monospace)
}

/// Arama için Türkçe karakterleri sadeleştirir: "İstanbul" ile "istanbul",
/// "Iğdır" ile "igdir" aynı sonucu verir.
fn arama_anahtari(metin: &str) -> String {
    metin
        .chars()
        .map(|c| match c {
            'İ' | 'I' | 'ı' | 'i' => 'i',
            'Ç' | 'ç' => 'c',
            'Ğ' | 'ğ' => 'g',
            'Ö' | 'ö' => 'o',
            'Ş' | 'ş' => 's',
            'Ü' | 'ü' => 'u',
            'Â' | 'â' => 'a',
            'Î' | 'î' => 'i',
            'Û' | 'û' => 'u',
            diger => diger.to_ascii_lowercase(),
        })
        .collect()
}

fn metin_genisligi(ui: &egui::Ui, metin: &str, boyut: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(metin.to_owned(), baslik_font(boyut), METIN)
        .size()
        .x
}

/// Windows'un sistem yazı tiplerini yükler: gövde için Segoe UI, başlıklar
/// için Segoe UI Semibold/Bold, günlük için Consolas. Herhangi biri yoksa
/// egui'nin gömülü yazı tipine düşer.
fn yazi_tipi_yukle(ctx: &egui::Context) {
    fn dosyadan(fonts: &mut egui::FontDefinitions, ad: &str, yol: &str) -> bool {
        match std::fs::read(yol) {
            Ok(veri) => {
                fonts
                    .font_data
                    .insert(ad.to_owned(), egui::FontData::from_owned(veri));
                true
            }
            Err(_) => false,
        }
    }

    let mut fonts = egui::FontDefinitions::default();

    let govde = dosyadan(&mut fonts, "seg_govde", r"C:\Windows\Fonts\segoeui.ttf");
    let yari = dosyadan(&mut fonts, "seg_yari", r"C:\Windows\Fonts\seguisb.ttf");
    let kalin = dosyadan(&mut fonts, "seg_kalin", r"C:\Windows\Fonts\segoeuib.ttf");
    let kod = dosyadan(&mut fonts, "consolas", r"C:\Windows\Fonts\consola.ttf");

    let varsayilan = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut orantili = varsayilan.clone();
    if govde {
        orantili.insert(0, "seg_govde".to_owned());
    }
    fonts
        .families
        .insert(egui::FontFamily::Proportional, orantili);

    let mut basliklar: Vec<String> = Vec::new();
    if kalin {
        basliklar.push("seg_kalin".to_owned());
    }
    if yari {
        basliklar.push("seg_yari".to_owned());
    }
    basliklar.extend(varsayilan);
    fonts
        .families
        .insert(egui::FontFamily::Name("Baslik".into()), basliklar);

    let mut mono = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    if kod {
        mono.insert(0, "consolas".to_owned());
    }
    fonts.families.insert(egui::FontFamily::Monospace, mono);

    ctx.set_fonts(fonts);
}

fn tema_uygula(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = ARKA_PLAN;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = GIRINTI;
    visuals.faint_bg_color = PANEL;
    visuals.code_bg_color = GIRINTI;
    visuals.override_text_color = Some(METIN);
    visuals.hyperlink_color = MAVI;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, KENAR);
    visuals.selection.bg_fill = VURGU.gamma_multiply(0.26);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, VURGU);
    visuals.window_rounding = egui::Rounding::same(YUVARLAK_KART);
    visuals.menu_rounding = egui::Rounding::same(10.0);

    let kenarli =
        |w: &mut egui::style::WidgetVisuals, dolgu: egui::Color32, kenar: egui::Color32| {
            w.bg_fill = dolgu;
            w.weak_bg_fill = dolgu;
            w.bg_stroke = egui::Stroke::new(1.0_f32, kenar);
            w.rounding = egui::Rounding::same(YUVARLAK_KUCUK);
            w.expansion = 0.0;
        };

    kenarli(&mut visuals.widgets.noninteractive, PANEL, KENAR);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, METIN);
    kenarli(&mut visuals.widgets.inactive, KART, KENAR);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, METIN);
    kenarli(&mut visuals.widgets.hovered, KART_SECILI, KENAR_GUCLU);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, METIN);
    kenarli(&mut visuals.widgets.active, VURGU, VURGU);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, KOYU_YAZI);
    kenarli(&mut visuals.widgets.open, KART, KENAR_GUCLU);
    // Kaydırma çubuğu tutamacı `widgets.inactive.bg_fill` rengini kullanır;
    // varsayılan buton çizimini kullanmadığımız için bu rengi burada seçiyoruz.
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(72, 82, 101);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(72, 82, 101);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.floating_width = 3.0;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.foreground_color = false;

    style.text_styles = [
        (egui::TextStyle::Heading, baslik_font(18.0)),
        (egui::TextStyle::Body, govde_font(13.0)),
        (egui::TextStyle::Button, govde_font(13.0)),
        (egui::TextStyle::Small, govde_font(11.0)),
        (egui::TextStyle::Monospace, kod_font(11.5)),
    ]
    .into();

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Ortak arayüz parçaları
// ---------------------------------------------------------------------------

/// Küçük, harf aralıklı bölüm etiketi ("FİLTRELER", "SONUÇLAR").
fn bolum_etiketi(ui: &mut egui::Ui, metin: &str) {
    ui.label(
        egui::RichText::new(metin)
            .font(baslik_font(10.0))
            .color(DIM),
    );
}

fn pill(ui: &mut egui::Ui, metin: &str, renk: egui::Color32) {
    egui::Frame::none()
        .fill(renk.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0_f32, renk.gamma_multiply(0.28)))
        .rounding(egui::Rounding::same(99.0))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(metin).size(10.0).color(renk));
        });
}

fn kontur_pill(ui: &mut egui::Ui, metin: &str) {
    let boy = egui::vec2(metin_genisligi(ui, metin, 10.5) + 24.0, 26.0);
    let (rect, _) = ui.allocate_exact_size(boy, egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(99.0), KART);
    p.rect_stroke(
        rect,
        egui::Rounding::same(99.0),
        egui::Stroke::new(1.0_f32, KENAR),
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        metin,
        govde_font(10.5),
        SOLUK,
    );
}

fn durum_pill(ui: &mut egui::Ui, calisiyor: bool) {
    let (renk, metin) = if calisiyor {
        (VURGU, "Taranıyor")
    } else {
        (YESIL, "Hazır")
    };
    egui::Frame::none()
        .fill(renk.gamma_multiply(0.10))
        .stroke(egui::Stroke::new(1.0_f32, renk.gamma_multiply(0.35)))
        .rounding(egui::Rounding::same(99.0))
        .inner_margin(egui::Margin::symmetric(11.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if calisiyor {
                    ui.add(egui::Spinner::new().size(11.0).color(renk));
                } else {
                    let (nokta, _) =
                        ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                    ui.painter().circle_filled(nokta.center(), 3.5, renk);
                }
                ui.label(egui::RichText::new(metin).size(11.5).color(renk));
            });
        });
}

/// Dolu (ana aksiyon) buton. `boy` verilmezse metne göre boyutlanır.
fn birincil_buton(
    ui: &mut egui::Ui,
    metin: &str,
    etkin: bool,
    boy: Option<egui::Vec2>,
) -> egui::Response {
    let boy = boy.unwrap_or_else(|| egui::vec2(metin_genisligi(ui, metin, 12.0) + 32.0, 30.0));
    let (rect, yanit) = ui.allocate_exact_size(
        boy,
        if etkin {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, etkin && yanit.hovered(), 0.12);
    let dolgu = if etkin {
        karistir(VURGU, VURGU_ACIK, t)
    } else {
        KART
    };
    let kenar = if etkin {
        karistir(VURGU, VURGU_ACIK, t)
    } else {
        KENAR
    };
    let yazi = if etkin { KOYU_YAZI } else { DIM };
    ui.painter()
        .rect_filled(rect, egui::Rounding::same(YUVARLAK_KUCUK), dolgu);
    ui.painter().rect_stroke(
        rect,
        egui::Rounding::same(YUVARLAK_KUCUK),
        egui::Stroke::new(1.0_f32, kenar),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        metin,
        baslik_font(12.0),
        yazi,
    );
    yanit
}

/// Çerçeveli ikincil buton.
fn ikincil_buton(
    ui: &mut egui::Ui,
    metin: &str,
    etkin: bool,
    boy: Option<egui::Vec2>,
) -> egui::Response {
    let boy = boy.unwrap_or_else(|| egui::vec2(metin_genisligi(ui, metin, 11.5) + 28.0, 30.0));
    let (rect, yanit) = ui.allocate_exact_size(
        boy,
        if etkin {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, etkin && yanit.hovered(), 0.12);
    let dolgu = karistir(egui::Color32::TRANSPARENT, KART, t);
    let kenar = karistir(KENAR, KENAR_GUCLU, t);
    let yazi = if !etkin {
        DIM
    } else {
        karistir(SOLUK, METIN, t)
    };
    ui.painter()
        .rect_filled(rect, egui::Rounding::same(YUVARLAK_KUCUK), dolgu);
    ui.painter().rect_stroke(
        rect,
        egui::Rounding::same(YUVARLAK_KUCUK),
        egui::Stroke::new(1.0_f32, kenar),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        metin,
        govde_font(11.5),
        yazi,
    );
    yanit
}

/// Dolgusuz, düşük vurgulu buton (Temizle, Tümünü seç...).
fn hayalet_buton(ui: &mut egui::Ui, metin: &str) -> egui::Response {
    let boy = egui::vec2(metin_genisligi(ui, metin, 11.5) + 20.0, 28.0);
    let (rect, yanit) = ui.allocate_exact_size(boy, egui::Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.12);
    if t > 0.01 {
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(8.0),
            karistir(egui::Color32::TRANSPARENT, KART, t),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        metin,
        govde_font(11.5),
        karistir(DIM, METIN, t),
    );
    yanit
}

/// Etiketli, çerçeveli arama alanı.
fn arama_kutusu(ui: &mut egui::Ui, metin: &mut String, ipucu: &str) {
    egui::Frame::none()
        .fill(GIRINTI)
        .stroke(egui::Stroke::new(1.0_f32, KENAR))
        .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(metin)
                    .frame(false)
                    .hint_text(egui::RichText::new(ipucu).size(11.5).color(DIM))
                    .font(govde_font(12.0))
                    .desired_width(f32::INFINITY),
            );
        });
}

/// Katalogdaki grup başlıkları, listelenme sırasıyla ve tekrarsız.
fn sektor_gruplari() -> Vec<&'static str> {
    let mut gruplar: Vec<&'static str> = Vec::new();
    for tanim in SEKTORLER {
        if gruplar.last() != Some(&tanim.grup) {
            gruplar.push(tanim.grup);
        }
    }
    gruplar
}

/// Bir gruptaki sektörler, katalog sırasıyla.
fn grup_sektorleri(grup: &str) -> Vec<&'static str> {
    SEKTORLER
        .iter()
        .filter(|tanim| tanim.grup == grup)
        .map(|tanim| tanim.ad)
        .collect()
}

/// Sektörün katalogdaki sırası; panelden seçimde kullanılır.
fn sektor_sirasi(ad: &str) -> usize {
    SEKTORLER
        .iter()
        .position(|tanim| tanim.ad == ad)
        .unwrap_or(0)
}

/// Metni verilen genişliğe sığdırır, taşarsa sonuna üç nokta koyar.
fn kisalt_metin(ui: &egui::Ui, metin: &str, genislik: f32, boyut: f32) -> String {
    let olc = |parca: &str| {
        ui.painter()
            .layout_no_wrap(parca.to_owned(), govde_font(boyut), METIN)
            .size()
            .x
    };
    if olc(metin) <= genislik {
        return metin.to_string();
    }
    let mut kisa = String::new();
    for karakter in metin.chars() {
        let deneme = format!("{kisa}{karakter}…");
        if olc(&deneme) > genislik {
            break;
        }
        kisa.push(karakter);
    }
    format!("{kisa}…")
}

/// Filtre panelindeki sektör özeti; tıklanınca seçim paneli açılır.
fn sektor_secici(ui: &mut egui::Ui, secili: &str) -> egui::Response {
    let genislik = ui.available_width();
    let (rect, yanit) = ui.allocate_exact_size(egui::vec2(genislik, 42.0), egui::Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.10);
    let p = ui.painter();
    p.rect_filled(
        rect,
        egui::Rounding::same(YUVARLAK_KUCUK),
        karistir(GIRINTI, KART_SECILI, t * 0.55),
    );
    p.rect_stroke(
        rect,
        egui::Rounding::same(YUVARLAK_KUCUK),
        egui::Stroke::new(1.0_f32, karistir(KENAR, KENAR_GUCLU, t)),
    );
    p.text(
        rect.left_center() + egui::vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        secili,
        govde_font(12.5),
        METIN,
    );
    p.text(
        rect.right_center() - egui::vec2(12.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        "Değiştir",
        govde_font(10.5),
        if yanit.hovered() { VURGU } else { SOLUK },
    );
    if yanit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    yanit
}

/// Panelin sol sütunundaki grup satırı: ad, sektör sayısı, seçili vurgusu.
fn grup_satiri(ui: &mut egui::Ui, ad: &str, sayi: usize, aktif: bool) -> egui::Response {
    let genislik = ui.available_width();
    let (rect, yanit) = ui.allocate_exact_size(egui::vec2(genislik, 23.0), egui::Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.10);
    if aktif {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(7.0), VURGU.gamma_multiply(0.14));
    } else if t > 0.0 {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(7.0), KENAR.gamma_multiply(t));
    }
    let etiket = kisalt_metin(ui, ad, genislik - 32.0, 11.0);
    ui.painter().text(
        rect.left_center() + egui::vec2(9.0, 0.0),
        egui::Align2::LEFT_CENTER,
        etiket,
        govde_font(11.0),
        if aktif {
            VURGU
        } else if yanit.hovered() {
            METIN
        } else {
            SOLUK
        },
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        sayi.to_string(),
        govde_font(9.5),
        DIM,
    );
    yanit
}

/// Panelin sağ sütunundaki sektör satırı; `alt` doluysa sağda grup adı yazar.
fn sektor_satiri(ui: &mut egui::Ui, ad: &str, alt: &str, secili: bool) -> egui::Response {
    let genislik = ui.available_width();
    let (rect, yanit) = ui.allocate_exact_size(egui::vec2(genislik, 24.0), egui::Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.10);
    if secili {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(7.0), VURGU.gamma_multiply(0.13));
        let cubuk = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(2.0, 6.0),
            egui::vec2(3.0, 12.0),
        );
        ui.painter()
            .rect_filled(cubuk, egui::Rounding::same(2.0), VURGU);
    } else if t > 0.0 {
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(7.0),
            KENAR.gamma_multiply(t * 0.8),
        );
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(13.0, 0.0),
        egui::Align2::LEFT_CENTER,
        ad,
        govde_font(12.0),
        if secili {
            VURGU
        } else if yanit.hovered() {
            METIN
        } else {
            SOLUK
        },
    );
    if !alt.is_empty() {
        ui.painter().text(
            rect.right_center() - egui::vec2(10.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            alt,
            govde_font(9.5),
            DIM,
        );
    }
    if yanit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    yanit
}

/// Seçilebilir liste satırı: solda ince vurgu çubuğu, yumuşak hover.
fn liste_satiri(ui: &mut egui::Ui, secili: bool, metin: &str) -> egui::Response {
    let genislik = ui.available_width();
    let (rect, yanit) = ui.allocate_exact_size(egui::vec2(genislik, 25.0), egui::Sense::click());

    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.10);
    let dolgu = if secili {
        VURGU.gamma_multiply(0.13)
    } else {
        karistir(egui::Color32::TRANSPARENT, KENAR, t * 0.7)
    };
    if dolgu != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(7.0), dolgu);
    }
    if secili {
        let cubuk = egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(2.0, 7.0),
            egui::vec2(3.0, 11.0),
        );
        ui.painter()
            .rect_filled(cubuk, egui::Rounding::same(2.0), VURGU);
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(13.0, 0.0),
        egui::Align2::LEFT_CENTER,
        metin,
        govde_font(12.0),
        if secili {
            VURGU
        } else if yanit.hovered() {
            METIN
        } else {
            SOLUK
        },
    );
    yanit
}

fn secim_dairesi(ui: &mut egui::Ui, secili: &mut bool) -> egui::Response {
    let (rect, mut yanit) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    if yanit.clicked() {
        *secili = !*secili;
        yanit.mark_changed();
    }
    let merkez = rect.center();
    if *secili {
        ui.painter().circle_filled(merkez, 8.5, VURGU);
        let onay = vec![
            merkez + egui::vec2(-3.7, 0.2),
            merkez + egui::vec2(-0.9, 3.0),
            merkez + egui::vec2(4.0, -3.0),
        ];
        ui.painter().add(egui::Shape::line(
            onay,
            egui::Stroke::new(1.7_f32, KOYU_YAZI),
        ));
    } else {
        let renk = if yanit.hovered() { KENAR_GUCLU } else { KENAR };
        ui.painter()
            .circle_stroke(merkez, 8.5, egui::Stroke::new(1.5_f32, renk));
    }
    yanit
}

fn istatistik_karti(
    ui: &mut egui::Ui,
    etiket: &str,
    deger: &str,
    renk: egui::Color32,
    genislik: f32,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(genislik, 76.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(YUVARLAK_KART), KART);
    p.rect_stroke(
        rect,
        egui::Rounding::same(YUVARLAK_KART),
        egui::Stroke::new(1.0_f32, KENAR),
    );
    let cubuk = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(14.0, 20.0),
        egui::vec2(3.0, 36.0),
    );
    p.rect_filled(cubuk, egui::Rounding::same(2.0), renk);
    p.text(
        rect.left_top() + egui::vec2(28.0, 17.0),
        egui::Align2::LEFT_TOP,
        etiket,
        baslik_font(10.0),
        DIM,
    );
    p.text(
        rect.left_top() + egui::vec2(28.0, 34.0),
        egui::Align2::LEFT_TOP,
        deger,
        baslik_font(24.0),
        METIN,
    );
}

fn log_rengi(satir: &str) -> egui::Color32 {
    if satir.starts_with("[!]") {
        KIRMIZI
    } else if satir.starts_with("[✓]") {
        YESIL
    } else if satir.starts_with("[i]") {
        MAVI
    } else {
        SOLUK
    }
}

fn avatar(ui: &mut egui::Ui, isim: &str, renk: egui::Color32) {
    avatar_boyut(ui, isim, renk, 20.0);
}

fn avatar_boyut(ui: &mut egui::Ui, isim: &str, renk: egui::Color32, yaricap: f32) {
    let kenar = yaricap * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(kenar, kenar), egui::Sense::hover());
    let merkez = rect.center();
    let p = ui.painter();
    p.circle_filled(merkez, yaricap, renk.gamma_multiply(0.12));
    p.circle_stroke(
        merkez,
        yaricap,
        egui::Stroke::new(1.0_f32, renk.gamma_multiply(0.26)),
    );
    let harf = isim
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    p.text(
        merkez,
        egui::Align2::CENTER_CENTER,
        harf,
        baslik_font(yaricap * 0.8),
        renk,
    );
}

/// Demo dosyasını üretir (yoksa) ve tarayıcıda açar.
fn demo_ac(lead: &mut BusinessLead, gonderen: &str, logs: &mut Vec<String>) {
    if lead.demo_yolu.is_none() {
        match demo_uret(lead, gonderen) {
            Ok(yol) => lead.demo_yolu = Some(yol),
            Err(hata) => logs.push(format!("[!] {hata}")),
        }
    }
    if let Some(yol) = lead.demo_yolu.clone() {
        if let Err(hata) = tarayicida_ac(&yol) {
            logs.push(format!("[!] Demo açılamadı: {hata}"));
        } else {
            logs.push(format!(
                "[i] {}: demo ile birlikte dijital karne de hazır → {}/{}",
                lead.name,
                DEMO_KLASORU,
                karne_dosya_adi(lead)
            ));
        }
    }
}

/// Hazır mesajla WhatsApp sohbetini açar; göndermeyi kullanıcı yapar.
fn whatsapp_ac(lead: &mut BusinessLead, gonderen: &str, logs: &mut Vec<String>) {
    if lead.durum.kara_listede() {
        logs.push(format!(
            "[!] {} mesaj istemedi (kara liste); WhatsApp açılmadı.",
            lead.name
        ));
        return;
    }
    let Some(numara) = whatsapp_numarasi(&lead.phone) else {
        return;
    };
    // Mesajda paylaşılabilir link olsun: yayınlanmamışsa önce demo yayınlanır.
    // Yayın başarısız olursa mesaj link olmadan açılır, gönderim engellenmez.
    if lead.yayin_linki.is_none() {
        yayina_gonder_ve_link_al(lead, gonderen, logs);
    }
    let mesaj = whatsapp_mesaji(lead);
    if sohbet_ac(&numara, &mesaj, logs) {
        lead.mesaj_hazir = true;
        logs.push(format!(
            "[✓] {} için WhatsApp sohbeti hazırlandı; mesajı siz gönderiyorsunuz.",
            lead.name
        ));
    }
}

/// Hazır metni WhatsApp sohbetinde açar; açılıp açılmadığını döndürür.
fn sohbet_ac(numara: &str, mesaj: &str, logs: &mut Vec<String>) -> bool {
    let url = format!("https://wa.me/{numara}?text={}", url_encode(mesaj));
    match tarayicida_ac(&url) {
        Ok(()) => true,
        Err(hata) => {
            logs.push(format!("[!] WhatsApp açılamadı: {hata}"));
            false
        }
    }
}

/// Yanıt bekleyen bir işletmeye gönderilecek kısa hatırlatma.
fn takip_mesaji(kayit: &musteriler::Musteri) -> String {
    let gecen = kayit
        .son_gonderim
        .as_deref()
        .or(kayit.son_temas.as_deref())
        .and_then(|tarih| musteriler::gun_farki(tarih, &bugun_anahtari()))
        .unwrap_or(0)
        .max(1);
    let mut metin = format!(
        "Merhaba, {gecen} gün önce {} için hazırladığım web sitesi taslağını paylaşmıştım. İnceleme fırsatınız oldu mu?",
        kayit.ad
    );
    if let Some(link) = kayit.yayin_linki.as_deref() {
        metin.push_str(&format!(" Taslak hâlâ yayında: {link}"));
    }
    metin.push_str(
        " İsterseniz alan adınızda 2 dakikada yayına alabilirim. İstemezseniz \"RET\" yazın, bir daha yazmam.",
    );
    metin
}

fn detay_satiri(ui: &mut egui::Ui, etiket: &str, deger: &str) {
    ui.label(
        egui::RichText::new(etiket)
            .font(baslik_font(9.5))
            .color(DIM),
    );
    ui.add_space(3.0);
    ui.label(egui::RichText::new(deger).size(12.0).color(METIN));
    ui.add_space(12.0);
}

fn kart_ciz(
    ui: &mut egui::Ui,
    lead: &mut BusinessLead,
    gonderen: &str,
    logs: &mut Vec<String>,
    acik: bool,
    limit_doldu: bool,
) -> (bool, bool) {
    let secili = lead.selected;
    let kategori_rengi = sektor_rengi(&lead.category);
    let mut detay_iste = false;
    let mut onizleme_iste = false;

    let kenar = if acik {
        MAVI.gamma_multiply(0.55)
    } else if secili {
        VURGU.gamma_multiply(0.4)
    } else {
        KENAR
    };

    let ic = egui::Frame::none()
        .fill(if secili || acik { KART_SECILI } else { KART })
        .stroke(egui::Stroke::new(1.0_f32, kenar))
        .rounding(egui::Rounding::same(YUVARLAK_KART))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            let genislik = ui.available_width();
            let wa_metin = if lead.mesaj_hazir {
                "Tekrar aç"
            } else {
                "WhatsApp"
            };
            let demo_metin = if lead.demo_yolu.is_some() {
                "Demoyu aç"
            } else {
                "Demo üret"
            };
            let butonlar_g = metin_genisligi(ui, wa_metin, 12.0)
                + 32.0
                + 6.0
                + metin_genisligi(ui, demo_metin, 11.5)
                + 28.0
                + 4.0;
            let bilgi_g = (genislik - butonlar_g).max(120.0);

            // Kartın aksiyon düğmeleri dışındaki alanı detay panelini açar.
            let bilgi_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(bilgi_g, 62.0));
            let kimlik = ui.make_persistent_id(("kart", &lead.kaynak, &lead.name));
            let tiklama = ui
                .interact(bilgi_rect, kimlik.with("tikla"), egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);

            ui.allocate_ui_with_layout(
                egui::vec2(genislik, 62.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    secim_dairesi(ui, &mut lead.selected);
                    ui.add_space(10.0);
                    avatar(ui, &lead.name, kategori_rengi);
                    ui.add_space(13.0);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(
                                egui::RichText::new(&lead.name)
                                    .font(baslik_font(14.5))
                                    .color(METIN),
                            );
                            pill(ui, &lead.category, kategori_rengi);
                            if lead.mesaj_hazir {
                                pill(ui, "mesaj hazır", VURGU);
                            }
                            if potansiyel_puani(lead) >= SICAK_ESIK {
                                pill(ui, "sıcak fırsat", YESIL);
                            }
                            match lead.site {
                                SiteDurumu::Sosyal => pill(ui, "sosyal medya var", MAVI),
                                SiteDurumu::Ucretsiz => pill(ui, "ücretsiz site", MAVI),
                                SiteDurumu::Yok => {}
                            }
                            if whatsapp_numarasi(&lead.phone).is_none() {
                                pill(ui, "telefon yok", KIRMIZI);
                            }
                        });
                        ui.label(egui::RichText::new(&lead.address).size(11.5).color(SOLUK));

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let mut alt_satir = String::new();
                            if !lead.phone.is_empty() {
                                alt_satir.push_str(&lead.phone);
                            }
                            if !lead.calisma_saatleri.is_empty() {
                                if !alt_satir.is_empty() {
                                    alt_satir.push_str("   ·   ");
                                }
                                alt_satir.push_str(&calisma_saatleri_metni(&lead.calisma_saatleri));
                            }
                            if !alt_satir.is_empty() {
                                ui.label(egui::RichText::new(alt_satir).size(11.0).color(DIM));
                                ui.add_space(12.0);
                            }
                            ui.hyperlink_to(
                                egui::RichText::new(kaynak_etiketi(&lead.kaynak))
                                    .size(10.5)
                                    .color(DIM),
                                &lead.kaynak,
                            );
                        });
                    });

                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 30.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let numara = whatsapp_numarasi(&lead.phone);
                            let wa = birincil_buton(
                                ui,
                                wa_metin,
                                numara.is_some() && !limit_doldu,
                                None,
                            );
                            let wa = if numara.is_none() {
                                wa.on_hover_text("Bu işletmenin telefon numarası kayıtta yok")
                            } else if limit_doldu {
                                wa.on_hover_text(format!(
                                    "Günlük gönderim hakkı doldu ({} mesaj); yarın devam edin",
                                    GUNLUK_GONDERIM_LIMITI
                                ))
                            } else {
                                wa.on_hover_text("Mesajı göndermeden önce önizleriniz")
                            };
                            if wa.clicked() {
                                onizleme_iste = true;
                            }

                            ui.add_space(6.0);
                            if ikincil_buton(ui, demo_metin, true, None).clicked() {
                                demo_ac(lead, gonderen, logs);
                            }
                        },
                    );
                },
            );

            if tiklama.clicked() {
                detay_iste = true;
            }
        });

    let rect = ic.response.rect;
    let yanit = ui.interact(
        rect,
        ui.make_persistent_id(("kart", &lead.kaynak, &lead.name))
            .with("hover"),
        egui::Sense::hover(),
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered() && !secili && !acik, 0.14);
    if t > 0.01 {
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(YUVARLAK_KART),
            egui::Color32::from_white_alpha((t * 6.0) as u8),
        );
    }

    (detay_iste, onizleme_iste)
}

impl LeadApp {
    /// Seçilen işletmenin ayrıntılarını ve aksiyonlarını sağ panelde gösterir.
    /// Günlük gönderim hakkı doldu mu?
    fn gunluk_limit_doldu(&self) -> bool {
        self.defter.gun_gonderim(&bugun_anahtari()) >= GUNLUK_GONDERIM_LIMITI
    }

    /// WhatsApp'ı açmadan önce yayın linkini tamamlar, mesajı üretir ve
    /// önizlemeyi açar. Gönderme kararı kullanıcının kalir.
    fn gonderime_hazirla(&mut self, idx: usize, gonderen: &str) {
        if idx >= self.leads.len() {
            return;
        }
        if self.leads[idx].durum.kara_listede() {
            self.logs.push(format!(
                "[!] {} mesaj istemedi (kara liste); WhatsApp açılmadı.",
                self.leads[idx].name
            ));
            return;
        }
        if whatsapp_numarasi(&self.leads[idx].phone).is_none() {
            self.logs.push(format!(
                "[!] {}: telefon numarası kayıtta yok, mesaj hazırlanamadı.",
                self.leads[idx].name
            ));
            return;
        }
        let bugun = bugun_anahtari();
        let anahtar = musteriler::anahtar_uret(
            &self.leads[idx].kaynak,
            &self.leads[idx].name,
            self.leads[idx].konum,
        );
        // Aynı işletmeye aynı gün tekrar mesaj hazırlamak haktan düşmez.
        if !self.defter.bugun_gonderildi_mi(&anahtar, &bugun) && self.gunluk_limit_doldu() {
            self.logs.push(format!(
                "[!] Günlük gönderim hakkı doldu ({}/{}). Hesabın askıya alınmaması için yarın devam edin.",
                GUNLUK_GONDERIM_LIMITI, GUNLUK_GONDERIM_LIMITI
            ));
            return;
        }
        if self.leads[idx].yayin_linki.is_none() {
            yayina_gonder_ve_link_al(&mut self.leads[idx], gonderen, &mut self.logs);
        }
        let mesaj = whatsapp_mesaji(&self.leads[idx]);
        self.onizleme = Some((idx, mesaj));
    }

    /// Önizlemedeki mesajı WhatsApp sohbetine aktarır ve gönderimi deftere işler.
    fn gonderimi_baslat(&mut self, idx: usize, gonderen: &str) {
        if idx >= self.leads.len() {
            return;
        }
        whatsapp_ac(&mut self.leads[idx], gonderen, &mut self.logs);
        self.gonderim_kaydet(idx);
    }

    /// Gönderimi deftere yazar; aynı işletme için günde bir kez sayılır.
    fn gonderim_kaydet(&mut self, idx: usize) {
        if idx >= self.leads.len() {
            return;
        }
        let anahtar = musteriler::anahtar_uret(
            &self.leads[idx].kaynak,
            &self.leads[idx].name,
            self.leads[idx].konum,
        );
        let bugun = bugun_anahtari();
        let onceki = self.defter.gun_gonderim(&bugun);
        self.defter.gonderim_kaydet(&anahtar, &bugun);
        let simdiki = self.defter.gun_gonderim(&bugun);
        if simdiki == onceki {
            return;
        }
        if let Some(link) = self.leads[idx].yayin_linki.clone() {
            self.defter.link_ayarla(&anahtar, &link);
        }
        self.defter.kaydet();
        self.logs.push(format!(
            "[i] Bugünkü gönderim: {simdiki}/{GUNLUK_GONDERIM_LIMITI}"
        ));
    }

    /// Gönderim öncesi mesaj önizlemesi: metin, uzunluk, link ve kalan hak.
    fn onizleme_paneli(&mut self, ctx: &egui::Context, gonderen: &str) {
        let Some((idx, mesaj)) = self.onizleme.clone() else {
            return;
        };
        if idx >= self.leads.len() {
            self.onizleme = None;
            return;
        }
        let ekran = ctx.screen_rect();
        let boyut = egui::vec2((ekran.width() - 240.0).clamp(430.0, 560.0), 448.0);
        let konum = ekran.center() - boyut * 0.5;
        let mut kapat = false;
        let mut gonder = false;
        let mut ret = false;

        egui::Area::new(egui::Id::new("onizleme_ortu"))
            .order(egui::Order::Foreground)
            .fixed_pos(ekran.min)
            .show(ctx, |ui| {
                let (rect, yanit) = ui.allocate_exact_size(ekran.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(155));
                if yanit.clicked() {
                    kapat = true;
                }
            });

        let kalan =
            GUNLUK_GONDERIM_LIMITI.saturating_sub(self.defter.gun_gonderim(&bugun_anahtari()));
        egui::Area::new(egui::Id::new("onizleme_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(konum)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(KART)
                    .stroke(egui::Stroke::new(1.0_f32, KENAR_GUCLU))
                    .rounding(egui::Rounding::same(16.0))
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0))
                    .show(ui, |ui| {
                        ui.set_width(boyut.x - 40.0);
                        ui.horizontal(|ui| {
                            bolum_etiketi(ui, "MESAJ ÖNİZLEMESİ");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if hayalet_buton(ui, "Kapat").clicked() {
                                        kapat = true;
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(&self.leads[idx].name)
                                .font(baslik_font(15.0))
                                .color(METIN),
                        );
                        ui.add_space(10.0);
                        egui::Frame::none()
                            .fill(GIRINTI)
                            .stroke(egui::Stroke::new(1.0_f32, KENAR))
                            .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
                            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                            .show(ui, |ui| {
                                ui.set_width(boyut.x - 64.0);
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(&mesaj).size(12.5).color(METIN),
                                        );
                                    });
                            });
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} karakter · {} link · bugün kalan hak: {kalan}",
                                mesaj.chars().count(),
                                mesaj.matches("https://").count()
                            ))
                            .size(10.5)
                            .color(DIM),
                        );
                        ui.add_space(12.0);
                        if birincil_buton(
                            ui,
                            "WhatsApp'ı aç ve gönder",
                            true,
                            Some(egui::vec2(boyut.x - 40.0, 36.0)),
                        )
                        .clicked()
                        {
                            gonder = true;
                        }
                        ui.add_space(6.0);
                        if ikincil_buton(
                            ui,
                            "İstemedi (RET) · kara listeye al",
                            true,
                            Some(egui::vec2(boyut.x - 40.0, 32.0)),
                        )
                        .clicked()
                        {
                            ret = true;
                        }
                    });
            });

        if gonder {
            self.gonderimi_baslat(idx, gonderen);
            kapat = true;
        }
        if ret {
            let anahtar = musteriler::anahtar_uret(
                &self.leads[idx].kaynak,
                &self.leads[idx].name,
                self.leads[idx].konum,
            );
            self.defter
                .durum_ayarla(&anahtar, musteriler::Durum::Reddetti, &bugun_anahtari());
            self.defter.kaydet();
            self.leads[idx].durum = musteriler::Durum::Reddetti;
            self.logs
                .push(format!("[i] {} → kara liste (RET).", self.leads[idx].name));
            kapat = true;
        }
        if kapat {
            self.onizleme = None;
        }
    }

    /// Takip kuyruğu: yanıt bekleyen ve üzerinden `TAKIP_GUN` gün geçmiş
    /// işletmeler; hepsine tek tuşla hatırlatma hazırlanabilir.
    fn takip_paneli(&mut self, ctx: &egui::Context) {
        if !self.takip_panel_acik {
            return;
        }
        let bugun = bugun_anahtari();
        let kayitlar: Vec<musteriler::Musteri> = self
            .defter
            .takip_edilecekler(&bugun, TAKIP_GUN)
            .into_iter()
            .cloned()
            .collect();
        let ekran = ctx.screen_rect();
        let boyut = egui::vec2(
            (ekran.width() - 200.0).clamp(560.0, 660.0),
            (ekran.height() - 200.0).clamp(360.0, 520.0),
        );
        let konum = ekran.center() - boyut * 0.5;
        let mut kapat = false;
        let mut acilacak: Option<String> = None;

        egui::Area::new(egui::Id::new("takip_ortu"))
            .order(egui::Order::Foreground)
            .fixed_pos(ekran.min)
            .show(ctx, |ui| {
                let (rect, yanit) = ui.allocate_exact_size(ekran.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(155));
                if yanit.clicked() {
                    kapat = true;
                }
            });

        egui::Area::new(egui::Id::new("takip_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(konum)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(KART)
                    .stroke(egui::Stroke::new(1.0_f32, KENAR_GUCLU))
                    .rounding(egui::Rounding::same(16.0))
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0))
                    .show(ui, |ui| {
                        ui.set_width(boyut.x - 40.0);
                        ui.set_height(boyut.y - 32.0);
                        ui.horizontal(|ui| {
                            bolum_etiketi(ui, "TAKİP KUYRUĞU");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if hayalet_buton(ui, "Kapat").clicked() {
                                        kapat = true;
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        // Huni: satışın hangi aşamada tıkandığını tek satırda gösterir.
                        let huni = self.defter.ozet();
                        let yanit = huni.ilgilenen + huni.kara_liste;
                        let ulasilan = huni.gonderilen + yanit;
                        let oran = (yanit * 100).checked_div(ulasilan).unwrap_or(0);
                        egui::Frame::none()
                            .fill(GIRINTI)
                            .stroke(egui::Stroke::new(1.0_f32, KENAR))
                            .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
                            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_width(boyut.x - 64.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Huni: {} kayıt · {} mesaj · {} yanıt (%{oran}) · {} ilgileniyor · {} kara liste",
                                        huni.toplam, ulasilan, yanit, huni.ilgilenen, huni.kara_liste
                                    ))
                                    .size(10.5)
                                    .color(SOLUK),
                                );
                            });
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} işletme {} gündür yanıt bekliyor · günlük kalan hak: {}",
                                kayitlar.len(),
                                TAKIP_GUN,
                                GUNLUK_GONDERIM_LIMITI
                                    .saturating_sub(self.defter.gun_gonderim(&bugun))
                            ))
                            .size(10.5)
                            .color(DIM),
                        );
                        ui.add_space(8.0);
                        if kayitlar.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Şu an takip sırası boş: gönderilen mesajların hepsi yanıtlanmış ya da henüz 3 gün dolmamış.",
                                )
                                .size(11.5)
                                .color(SOLUK),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for kayit in &kayitlar {
                                    let gecen = kayit
                                        .son_gonderim
                                        .as_deref()
                                        .or(kayit.son_temas.as_deref())
                                        .and_then(|tarih| {
                                            musteriler::gun_farki(tarih, &bugun)
                                        })
                                        .unwrap_or(0);
                                    egui::Frame::none()
                                        .fill(GIRINTI)
                                        .stroke(egui::Stroke::new(1.0_f32, KENAR))
                                        .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
                                        .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 3.0;
                                                    ui.label(
                                                        egui::RichText::new(&kayit.ad)
                                                            .size(12.5)
                                                            .color(METIN),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "{} · {} · {gecen} gün önce{}",
                                                            kayit.kategori,
                                                            if kayit.telefon.is_empty() {
                                                                "telefon yok"
                                                            } else {
                                                                &kayit.telefon
                                                            },
                                                            if kayit.notlar.is_empty() {
                                                                String::new()
                                                            } else {
                                                                format!(" · not: {}", kayit.notlar)
                                                            }
                                                        ))
                                                        .size(10.5)
                                                        .color(DIM),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let numara =
                                                            whatsapp_numarasi(&kayit.telefon);
                                                        let dugme = ikincil_buton(
                                                            ui,
                                                            "Hatırlat",
                                                            numara.is_some(),
                                                            Some(egui::vec2(86.0, 26.0)),
                                                        );
                                                        if dugme.clicked() {
                                                            acilacak =
                                                                Some(kayit.anahtar.clone());
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    ui.add_space(6.0);
                                }
                            });
                    });
            });

        if let Some(anahtar) = acilacak {
            self.takip_sohbet_ac(&anahtar, &bugun);
        }
        if kapat {
            self.takip_panel_acik = false;
        }
    }

    /// Takip kuyruğundaki bir işletmeye hatırlatma mesajı hazırlar.
    fn takip_sohbet_ac(&mut self, anahtar: &str, bugun: &str) {
        let Some(kayit) = self.defter.kayit(anahtar).cloned() else {
            return;
        };
        let Some(numara) = whatsapp_numarasi(&kayit.telefon) else {
            self.logs
                .push(format!("[!] {}: telefon numarası kayıtta yok.", kayit.ad));
            return;
        };
        if self.defter.gun_gonderim(bugun) >= GUNLUK_GONDERIM_LIMITI {
            self.logs.push(format!(
                "[!] Günlük gönderim hakkı doldu ({}/{}). Yarın devam edin.",
                GUNLUK_GONDERIM_LIMITI, GUNLUK_GONDERIM_LIMITI
            ));
            return;
        }
        let mesaj = takip_mesaji(&kayit);
        if sohbet_ac(&numara, &mesaj, &mut self.logs) {
            self.defter.gonderim_kaydet(anahtar, bugun);
            self.defter.kaydet();
            self.logs.push(format!(
                "[✓] {} için takip mesajı hazır; günlük hak {}/{GUNLUK_GONDERIM_LIMITI}.",
                kayit.ad,
                self.defter.gun_gonderim(bugun)
            ));
        }
    }

    fn detay_paneli(&mut self, ctx: &egui::Context, gonderen: &str) {
        let Some(idx) = self.detay_idx else {
            return;
        };
        if idx >= self.leads.len() {
            self.detay_idx = None;
            return;
        }

        let mut kapat = false;
        let mut yeni_durum: Option<musteriler::Durum> = None;
        let mut wa_iste = false;
        // Görüşme notu: panelde düzenlenir, değişince deftere yazılır.
        let mut not_metni = self
            .defter
            .kayit(&musteriler::anahtar_uret(
                &self.leads[idx].kaynak,
                &self.leads[idx].name,
                self.leads[idx].konum,
            ))
            .map(|kayit| kayit.notlar.clone())
            .unwrap_or_default();
        let mut not_degisti = false;
        // Defterdeki son temas tarihi panelde gösterilir (iptal öncesi okunur).
        let anahtar = musteriler::anahtar_uret(
            &self.leads[idx].kaynak,
            &self.leads[idx].name,
            self.leads[idx].konum,
        );
        let son_temas = self
            .defter
            .kayit(&anahtar)
            .and_then(|kayit| kayit.son_temas.clone());
        let son_gonderim = self
            .defter
            .kayit(&anahtar)
            .and_then(|kayit| kayit.son_gonderim.clone());
        // A/B sayaçları panelde tek satırda gösterilir.
        let varyant_ozeti = self.defter.varyant_ozeti();
        let sektor_ozeti = self
            .defter
            .varyant_ozeti_kategori(&self.leads[idx].category);
        // 30 gönderimin altında oran farkı gürültüdür; panel bunu açıkça söyler.
        let orneklem_yetersiz = varyant_ozeti.iter().any(|ozet| ozet.gonderim > 0)
            && !varyant_ozeti.iter().all(|ozet| ozet.anlamli());
        egui::SidePanel::right("detay_paneli")
            .resizable(false)
            .exact_width(348.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(18.0, 16.0)),
            )
            .show(ctx, |ui| {
                // Sağ panelde içerik genişliği sabit: sağ boşluk her zaman korunur.
                const IC_GENISLIK: f32 = 310.0;
                let lead = &mut self.leads[idx];
                let renk = sektor_rengi(&lead.category);

                ui.allocate_ui_with_layout(
                    egui::vec2(IC_GENISLIK, 26.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        bolum_etiketi(ui, "İŞLETME DETAYI");
                        let kalan = ui.available_width();
                        ui.add_space((kalan - 62.0).max(0.0));
                        if hayalet_buton(ui, "Kapat").clicked() {
                            kapat = true;
                        }
                    },
                );
                ui.add_space(10.0);

                // WhatsApp + demo + karne + yayın düğmeleri ve kaynak bağlantısı.
                let aksiyon_yuksekligi = 242.0;
                let kaydirma = (ui.available_height() - aksiyon_yuksekligi).max(140.0);
                egui::ScrollArea::vertical()
                    .max_height(kaydirma)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(IC_GENISLIK);

                        ui.horizontal(|ui| {
                            avatar_boyut(ui, &lead.name, renk, 26.0);
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 5.0;
                                ui.label(
                                    egui::RichText::new(&lead.name)
                                        .font(baslik_font(17.0))
                                        .color(METIN),
                                );
                                ui.horizontal(|ui| {
                                    pill(ui, &lead.category, renk);
                                    if lead.mesaj_hazir {
                                        pill(ui, "mesaj hazır", VURGU);
                                    }
                                    if potansiyel_puani(lead) >= SICAK_ESIK {
                                        pill(ui, "sıcak fırsat", YESIL);
                                    }
                                    match lead.site {
                                        SiteDurumu::Sosyal => pill(ui, "sosyal medya var", MAVI),
                                        SiteDurumu::Ucretsiz => pill(ui, "ücretsiz site", MAVI),
                                        SiteDurumu::Yok => {}
                                    }
                                    if whatsapp_numarasi(&lead.phone).is_none() {
                                        pill(ui, "telefon yok", KIRMIZI);
                                    }
                                });
                            });
                        });
                        ui.add_space(14.0);

                        if let Some(durum) = whatsapp_durumu(lead) {
                            let cumle = durum_kisa_cumle(&durum);
                            let durum_rengi = if cumle.starts_with("Şu an açık") {
                                YESIL
                            } else {
                                KIRMIZI
                            };
                            egui::Frame::none()
                                .fill(durum_rengi.gamma_multiply(0.10))
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    durum_rengi.gamma_multiply(0.30),
                                ))
                                .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
                                .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                                .show(ui, |ui| {
                                    ui.set_width(IC_GENISLIK - 22.0);
                                    ui.horizontal(|ui| {
                                        let (nokta, _) = ui.allocate_exact_size(
                                            egui::vec2(7.0, 7.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().circle_filled(
                                            nokta.center(),
                                            3.0,
                                            durum_rengi,
                                        );
                                        ui.label(
                                            egui::RichText::new(cumle)
                                                .size(11.5)
                                                .color(durum_rengi),
                                        );
                                    });
                                });
                            ui.add_space(14.0);
                        }

                        detay_satiri(ui, "SATIN ALMA POTANSİYELİ", &potansiyel_gerekcesi(lead));
                        detay_satiri(ui, "MÜŞTERİ DURUMU", lead.durum.etiket());
                        // A/B: hangi varyant gitti ve yanıt durumu ne âlemde.
                        let varyant = lead.mesaj_varyanti.unwrap_or_default();
                        let yanit = match lead.durum {
                            musteriler::Durum::Yeni | musteriler::Durum::MesajHazir => {
                                "henüz gönderilmedi"
                            }
                            musteriler::Durum::Gonderildi => "yanıt bekleniyor",
                            musteriler::Durum::Ilgilendi => "olumlu yanıt",
                            musteriler::Durum::Reddetti => "olumsuz yanıt",
                        };
                        detay_satiri(
                            ui,
                            "MESAJ VARYANTI",
                            &format!("{} · {yanit}", varyant.etiket()),
                        );
                        ui.horizontal(|ui| {
                            let dugme = |ui: &mut egui::Ui, metin: &str| {
                                ikincil_buton(ui, metin, true, Some(egui::vec2(94.0, 28.0))).clicked()
                            };
                            if dugme(ui, "Gönderdim") {
                                yeni_durum = Some(musteriler::Durum::Gonderildi);
                            }
                            if dugme(ui, "İlgileniyor") {
                                yeni_durum = Some(musteriler::Durum::Ilgilendi);
                            }
                            if dugme(ui, "Reddetti") {
                                yeni_durum = Some(musteriler::Durum::Reddetti);
                            }
                        });
                        if lead.durum.kara_listede() {
                            ui.label(
                                egui::RichText::new(
                                    "Kara listede: bu işletmeye bir daha mesaj hazırlanmaz.",
                                )
                                .size(11.0)
                                .color(KIRMIZI),
                            );
                        } else if let Some(temas) = &son_temas {
                            ui.label(
                                egui::RichText::new(format!("İlk temas: {temas}"))
                                    .size(10.5)
                                    .color(DIM),
                            );
                        }
                        if varyant_ozeti.iter().any(|ozet| ozet.gonderim > 0) {
                            let satirlar: Vec<String> = musteriler::Varyant::HEPSI
                                .iter()
                                .map(|varyant| varyant_ozeti[varyant.sira()].satir())
                                .collect();
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("A/B: {}", satirlar.join("  ·  ")))
                                    .size(10.5)
                                    .color(DIM),
                            );
                            if orneklem_yetersiz {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Örneklem yetersiz: kol başına {}+ gönderim gerekir.",
                                        musteriler::ANLAMLI_ORNEKLEM
                                    ))
                                    .size(10.0)
                                    .color(VURGU),
                                );
                            }
                        }
                        if sektor_ozeti.iter().any(|ozet| ozet.gonderim > 0) {
                            let satirlar: Vec<String> = musteriler::Varyant::HEPSI
                                .iter()
                                .map(|varyant| sektor_ozeti[varyant.sira()].satir())
                                .collect();
                            ui.label(
                                egui::RichText::new(format!(
                                    "A/B · {}: {}",
                                    lead.category,
                                    satirlar.join("  ·  ")
                                ))
                                .size(10.5)
                                .color(DIM),
                            );
                        }
                        if let Some(tarih) = &son_gonderim {
                            let gecen = musteriler::gun_farki(tarih, &bugun_anahtari())
                                .map(|gun| format!("{gun} gün önce"))
                                .unwrap_or_else(|| tarih.clone());
                            ui.label(
                                egui::RichText::new(format!("Son gönderim: {tarih} · {gecen}"))
                                    .size(10.5)
                                    .color(DIM),
                            );
                        }
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("GÖRÜŞME NOTU")
                                .font(baslik_font(9.5))
                                .color(DIM),
                        );
                        ui.add_space(4.0);
                        let duzenleme = egui::TextEdit::multiline(&mut not_metni)
                            .desired_rows(2)
                            .desired_width(IC_GENISLIK - 8.0)
                            .font(egui::TextStyle::Small)
                            .hint_text("Ör. fiyat sordu, pazartesi arayacak");
                        if ui.add(duzenleme).changed() {
                            not_degisti = true;
                        }
                        ui.add_space(10.0);
                        ui.add_space(12.0);
                        if !lead.site_linki.is_empty() {
                            let baslik = match lead.site {
                                SiteDurumu::Sosyal => "SOSYAL MEDYA",
                                SiteDurumu::Ucretsiz => "ÜCRETSİZ SİTE",
                                SiteDurumu::Yok => "WEB ADRESİ",
                            };
                            ui.label(
                                egui::RichText::new(baslik)
                                    .font(baslik_font(9.5))
                                    .color(DIM),
                            );
                            ui.add_space(3.0);
                            let gosterim = if lead.site_linki.chars().count() > 44 {
                                format!("{}…", lead.site_linki.chars().take(44).collect::<String>())
                            } else {
                                lead.site_linki.clone()
                            };
                            ui.hyperlink_to(
                                egui::RichText::new(gosterim).size(12.0).color(MAVI),
                                &lead.site_linki,
                            );
                            ui.add_space(12.0);
                        }
                        detay_satiri(ui, "ADRES", &lead.address);
                        detay_satiri(
                            ui,
                            "TELEFON",
                            if lead.phone.is_empty() {
                                "Kayıtta yok"
                            } else {
                                &lead.phone
                            },
                        );
                        let saatler = if lead.calisma_saatleri.is_empty() {
                            "Kayıtta yok".to_string()
                        } else {
                            calisma_saatleri_metni(&lead.calisma_saatleri)
                        };
                        detay_satiri(ui, "ÇALIŞMA SAATLERİ", &saatler);

                        ui.label(
                            egui::RichText::new("KONUM")
                                .font(baslik_font(9.5))
                                .color(DIM),
                        );
                        ui.add_space(3.0);
                        if let Some((enlem, boylam)) = lead.konum {
                            ui.label(
                                egui::RichText::new(format!("{enlem:.5}, {boylam:.5}"))
                                    .size(12.0)
                                    .color(METIN),
                            );
                            ui.add_space(2.0);
                            let harita = format!(
                                "https://www.openstreetmap.org/?mlat={enlem}&mlon={boylam}#map=18/{enlem}/{boylam}"
                            );
                            ui.hyperlink_to(
                                egui::RichText::new("Haritada aç ↗").size(11.0).color(MAVI),
                                harita,
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Koordinat kayıtta yok")
                                    .size(12.0)
                                    .color(DIM),
                            );
                        }
                        ui.add_space(12.0);

                        let (cizgi, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 1.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(cizgi, 0.0, KENAR);
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            secim_dairesi(ui, &mut lead.selected);
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(if lead.selected {
                                    "Bu işletme seçildi"
                                } else {
                                    "Bu işletmeyi seç"
                                })
                                .size(11.5)
                                .color(SOLUK),
                            );
                        });
                    });

                ui.add_space(10.0);
                let numara = whatsapp_numarasi(&lead.phone);
                let wa = birincil_buton(
                    ui,
                    if lead.mesaj_hazir {
                        "WhatsApp'ı yeniden aç"
                    } else {
                        "WhatsApp mesajını hazırla"
                    },
                    numara.is_some(),
                    Some(egui::vec2(IC_GENISLIK, 38.0)),
                );
                let wa = if numara.is_some() {
                    wa
                } else {
                    wa.on_hover_text("Telefon numarası kayıtta yok")
                };
                if wa.clicked() {
                    wa_iste = true;
                }

                ui.add_space(8.0);
                let demo_metin = if lead.demo_yolu.is_some() {
                    "Demoyu aç"
                } else {
                    "Demo üret"
                };
                if ikincil_buton(ui, demo_metin, true, Some(egui::vec2(IC_GENISLIK, 38.0))).clicked()
                {
                    demo_ac(lead, gonderen, &mut self.logs);
                }

                ui.add_space(8.0);
                if ikincil_buton(ui, "Dijital karne", true, Some(egui::vec2(IC_GENISLIK, 38.0)))
                    .on_hover_text("Eksikleri ve puanı tek sayfada gösterir; demo ile birlikte üretilir")
                    .clicked()
                {
                    karne_ac(lead, gonderen, &mut self.logs);
                }

                ui.add_space(8.0);
                if ikincil_buton(
                    ui,
                    if lead.yayin_linki.is_some() {
                        "Yayın linkini yenile"
                    } else {
                        "Demo linki oluştur"
                    },
                    true,
                    Some(egui::vec2(IC_GENISLIK, 38.0)),
                )
                .on_hover_text("Demoyu GitHub Pages'e yayınlar; linki müşteriye gönderebilirsiniz")
                .clicked()
                {
                    demo_yayinla(lead, gonderen, &mut self.logs);
                }
                if let Some(link) = lead.yayin_linki.clone() {
                    ui.add_space(2.0);
                    ui.hyperlink_to(
                        egui::RichText::new("Yayındaki demo linki")
                            .size(10.5)
                            .color(MAVI),
                        link,
                    );
                }
                ui.add_space(2.0);
                ui.hyperlink_to(
                    egui::RichText::new(kaynak_etiketi(&lead.kaynak))
                        .size(10.5)
                        .color(DIM),
                    &lead.kaynak,
                );
            });

        if not_degisti {
            self.defter.not_ayarla(&anahtar, &not_metni);
            self.defter.kaydet();
        }
        if wa_iste {
            self.gonderime_hazirla(idx, gonderen);
        }
        if let Some(durum) = yeni_durum {
            let lead = &self.leads[idx];
            let anahtar = musteriler::anahtar_uret(&lead.kaynak, &lead.name, lead.konum);
            self.defter.durum_ayarla(&anahtar, durum, &bugun_anahtari());
            self.defter.kaydet();
            // "Gönderdim" onayı günlük gönderim hakkından düşer.
            if durum == musteriler::Durum::Gonderildi {
                self.gonderim_kaydet(idx);
            }
            self.leads[idx].durum = durum;
            self.logs
                .push(format!("[i] {} → {}", self.leads[idx].name, durum.etiket()));
        }
        if kapat {
            self.detay_idx = None;
        }
    }

    /// Seçili il, ilçe (ya da il geneli) ve sektörle yeni bir tarama başlatır.
    /// Takip zamanı gelmiş işletmelerin defter anahtarları: mesaj gönderileli
    /// `TAKIP_GUN` günden fazla olmuş, hâlâ yanıt bekleyenler.
    fn takip_bekleyenler(&self, bugun: &str) -> HashSet<String> {
        self.defter
            .kayitlar()
            .iter()
            .filter(|kayit| kayit.durum == musteriler::Durum::Gonderildi)
            .filter(|kayit| {
                kayit
                    .son_temas
                    .as_deref()
                    .and_then(|temas| gun_farki(temas, bugun))
                    .map(|gun| gun >= TAKIP_GUN)
                    .unwrap_or(false)
            })
            .map(|kayit| kayit.anahtar.clone())
            .collect()
    }

    fn tarama_baslat(&mut self) {
        if self.is_running {
            return;
        }
        self.is_running = true;
        self.leads.clear();
        self.detay_idx = None;
        self.bos_ilce_uyari = None;
        self.ilerleme = None;
        let tx = self.log_tx.clone();
        let il = self.cities[self.selected_city_idx].0.to_string();
        let ilce = if self.il_geneli {
            None
        } else {
            Some(self.cities[self.selected_city_idx].1[self.selected_district_idx].to_string())
        };
        let sektor = self.sectors[self.selected_sector_idx].to_string();
        let kaynak = self.kaynak;
        thread::spawn(move || tarama_calistir(tx, il, ilce, sektor, kaynak));
    }

    /// İki sütunlu sektör seçim paneli: solda tüm gruplar, sağda seçili grubun
    /// sektörleri. Arama kutusu tüm katalogda arar.
    fn sektor_paneli(&mut self, ctx: &egui::Context) {
        if !self.sektor_panel_acik {
            return;
        }
        let ekran = ctx.screen_rect();
        let boyut = egui::vec2(
            (ekran.width() - 140.0).clamp(760.0, 920.0),
            (ekran.height() - 170.0).clamp(430.0, 580.0),
        );
        let konum = ekran.center() - boyut * 0.5;
        let mut kapat = false;

        // Arkaya tıklayınca kapanan karartma; panellerin üstünde kalmalı.
        egui::Area::new(egui::Id::new("sektor_ortu"))
            .order(egui::Order::Foreground)
            .fixed_pos(ekran.min)
            .show(ctx, |ui| {
                let (rect, yanit) = ui.allocate_exact_size(ekran.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(155));
                if yanit.clicked() {
                    kapat = true;
                }
            });

        egui::Area::new(egui::Id::new("sektor_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(konum)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(KART)
                    .stroke(egui::Stroke::new(1.0_f32, KENAR_GUCLU))
                    .rounding(egui::Rounding::same(16.0))
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0))
                    .show(ui, |ui| {
                        ui.set_width(boyut.x - 40.0);
                        ui.set_height(boyut.y - 32.0);
                        self.sektor_paneli_icerik(ui, &mut kapat);
                    });
            });

        if kapat || ctx.input(|girdi| girdi.key_pressed(egui::Key::Escape)) {
            self.sektor_panel_acik = false;
            self.sector_search.clear();
        }
    }

    fn sektor_paneli_icerik(&mut self, ui: &mut egui::Ui, kapat: &mut bool) {
        let gruplar = sektor_gruplari();
        if self.sektor_panel_grup >= gruplar.len() {
            self.sektor_panel_grup = 0;
        }

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Sektör seç")
                    .font(baslik_font(16.0))
                    .color(METIN),
            );
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} grup · {} sektör",
                    gruplar.len(),
                    SEKTORLER.len()
                ))
                .size(10.5)
                .color(DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if hayalet_buton(ui, "Kapat").clicked() {
                    *kapat = true;
                }
            });
        });
        ui.add_space(8.0);
        arama_kutusu(
            ui,
            &mut self.sector_search,
            "Sektör ara (kuaför, matbaa, fayans...)",
        );
        ui.add_space(10.0);

        let govde = (ui.available_height() - 2.0).max(220.0);
        let sol_genislik = 340.0_f32.min(ui.available_width() * 0.45);
        let mut secilen_grup: Option<usize> = None;

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(sol_genislik, govde),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    bolum_etiketi(ui, "GRUPLAR");
                    ui.add_space(6.0);
                    let aktif = gruplar[self.sektor_panel_grup];
                    egui::ScrollArea::vertical()
                        .id_source("panel_gruplar")
                        .max_height(govde - 22.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.columns(2, |kolonlar| {
                                for (idx, grup) in gruplar.iter().enumerate() {
                                    let sayi = SEKTORLER
                                        .iter()
                                        .filter(|tanim| tanim.grup == *grup)
                                        .count();
                                    if grup_satiri(
                                        &mut kolonlar[idx % 2],
                                        grup,
                                        sayi,
                                        *grup == aktif,
                                    )
                                    .clicked()
                                    {
                                        secilen_grup = Some(idx);
                                    }
                                }
                            });
                        });
                },
            );

            ui.add_space(18.0);

            let sag_genislik = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(sag_genislik, govde),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::Frame::none()
                        .fill(GIRINTI)
                        .stroke(egui::Stroke::new(1.0_f32, KENAR))
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::symmetric(6.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_height(govde - 2.0);
                            let sorgu = arama_anahtari(&self.sector_search);
                            let mut secilen_sektor: Option<usize> = None;

                            if sorgu.is_empty() {
                                let grup = gruplar[self.sektor_panel_grup];
                                let sektorler = grup_sektorleri(grup);
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(grup)
                                            .font(baslik_font(12.5))
                                            .color(METIN),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} sektör",
                                                    sektorler.len()
                                                ))
                                                .size(10.0)
                                                .color(DIM),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(8.0);
                                // Grup genişledikçe liste panelin dışına taşabilir:
                                // sektör listesi kaydırılabilir olmalı.
                                egui::ScrollArea::vertical()
                                    .id_source("panel_grup_sektorler")
                                    .max_height(govde - 56.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        for ad in sektorler {
                                            let secili =
                                                self.sectors[self.selected_sector_idx] == ad;
                                            if sektor_satiri(ui, ad, "", secili).clicked() {
                                                secilen_sektor = Some(sektor_sirasi(ad));
                                            }
                                        }
                                    });
                            } else {
                                let bulunanlar: Vec<&'static str> = SEKTORLER
                                    .iter()
                                    .filter(|tanim| arama_anahtari(tanim.ad).contains(&sorgu))
                                    .map(|tanim| tanim.ad)
                                    .collect();
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new("Arama")
                                            .font(baslik_font(12.5))
                                            .color(METIN),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} sonuç",
                                                    bulunanlar.len()
                                                ))
                                                .size(10.0)
                                                .color(DIM),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(8.0);
                                egui::ScrollArea::vertical()
                                    .id_source("panel_arama")
                                    .max_height(govde - 56.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        for ad in &bulunanlar {
                                            let secili =
                                                self.sectors[self.selected_sector_idx] == *ad;
                                            if sektor_satiri(ui, ad, sektor_grubu(ad), secili)
                                                .clicked()
                                            {
                                                secilen_sektor = Some(sektor_sirasi(ad));
                                            }
                                        }
                                        if bulunanlar.is_empty() {
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new("Eşleşen sektör yok")
                                                    .size(11.0)
                                                    .color(DIM),
                                            );
                                        }
                                    });
                            }

                            if let Some(idx) = secilen_sektor {
                                self.selected_sector_idx = idx;
                                self.bos_ilce_uyari = None;
                                *kapat = true;
                            }
                        });
                },
            );
        });

        if let Some(idx) = secilen_grup {
            self.sektor_panel_grup = idx;
        }
    }

    fn whatsapp_ayarlari(&mut self, ui: &mut egui::Ui) {
        bolum_etiketi(ui, "WHATSAPP");
        ui.add_space(6.0);

        let bagli = self.is_whatsapp_connected;
        let numara = whatsapp_numarasi(&self.whatsapp_phone);

        if !self.whatsapp_acik {
            egui::Frame::none()
                .fill(KART)
                .stroke(egui::Stroke::new(1.0_f32, KENAR))
                .rounding(egui::Rounding::same(YUVARLAK_KUCUK))
                .inner_margin(egui::Margin::symmetric(11.0, 9.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (nokta, _) =
                            ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            nokta.center(),
                            3.5,
                            if bagli { YESIL } else { DIM },
                        );
                        ui.add_space(3.0);
                        let gorunen_numara = if numara.is_some() {
                            self.whatsapp_phone.trim().to_string()
                        } else {
                            "Numara girilmedi".to_string()
                        };
                        ui.label(
                            egui::RichText::new(gorunen_numara)
                                .size(11.5)
                                .color(if bagli { METIN } else { SOLUK }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if hayalet_buton(ui, "Düzenle").clicked() {
                                self.whatsapp_acik = true;
                            }
                        });
                    });
                });
            return;
        }

        egui::Frame::none()
            .fill(KART)
            .stroke(egui::Stroke::new(1.0_f32, KENAR))
            .rounding(egui::Rounding::same(YUVARLAK_KART))
            .inner_margin(egui::Margin::symmetric(12.0, 11.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Gönderen numara")
                            .size(10.5)
                            .color(SOLUK),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if hayalet_buton(ui, "Kapat").clicked() {
                                self.whatsapp_acik = false;
                            }
                        },
                    );
                });
                ui.add_space(4.0);
                arama_kutusu(ui, &mut self.whatsapp_phone, "+90 532 123 45 67");
                ui.add_space(8.0);

                let dogrula_genislik = ui.available_width();
                let dogrula = ikincil_buton(
                    ui,
                    if self.is_whatsapp_connected {
                        "Numara doğrulandı"
                    } else {
                        "Numarayı doğrula"
                    },
                    true,
                    Some(egui::vec2(dogrula_genislik, 30.0)),
                );
                if dogrula.clicked() {
                    match whatsapp_numarasi(&self.whatsapp_phone) {
                        Some(numara) => {
                            self.is_whatsapp_connected = true;
                            self.whatsapp_status = format!("Hazır: +{numara}");
                            self.logs.push(format!(
                                "[✓] Numara doğrulandı: +{numara}. Mesajlar tek tıkla hazırlanır, göndermeyi siz onaylarsınız."
                            ));
                        }
                        None => {
                            self.is_whatsapp_connected = false;
                            self.whatsapp_status = "Geçersiz numara".to_string();
                            self.logs
                                .push("[!] Numara geçersiz. Örnek: +90 532 123 45 67".to_string());
                        }
                    }
                }

                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    let renk = if self.is_whatsapp_connected { YESIL } else { DIM };
                    let (nokta, _) =
                        ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                    ui.painter().circle_filled(nokta.center(), 3.5, renk);
                    ui.label(
                        egui::RichText::new(&self.whatsapp_status)
                            .size(11.0)
                            .color(renk),
                    );
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Otomatik gönderim yok; hesabınız askıya alınmaz.")
                        .size(10.0)
                        .color(DIM),
                );
            });
    }
}
impl eframe::App for LeadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(mesaj) = self.log_rx.try_recv() {
            match mesaj {
                Mesaj::Log(satir) => self.logs.push(satir),
                Mesaj::Ilerleme { tamamlanan, toplam } => {
                    self.ilerleme = Some((tamamlanan, toplam));
                }
                Mesaj::TaramaBitti(mut bulunanlar) => {
                    self.is_running = false;
                    self.ilerleme = None;
                    self.detay_idx = None;
                    let adet = bulunanlar.len();
                    // Kalıcı deftere işle: mükerrer kayıt açılmaz, daha önce
                    // temas edilmiş veya kara listedeki işletmenin durumu gelir.
                    let bugun = bugun_anahtari();
                    let mut bilinen = 0usize;
                    for lead in bulunanlar.iter_mut() {
                        let anahtar =
                            musteriler::anahtar_uret(&lead.kaynak, &lead.name, lead.konum);
                        let durum = self.defter.isle(
                            &anahtar,
                            &lead.name,
                            &lead.phone,
                            &lead.address,
                            &lead.category,
                            &lead.kaynak,
                            site_kodu(lead.site),
                            &lead.site_linki,
                            potansiyel_puani(lead),
                            &bugun,
                        );
                        if durum != musteriler::Durum::Yeni {
                            bilinen += 1;
                        }
                        lead.durum = durum;
                        // Mesaj varyantı defterde tutulur; aynı işletmeye hep aynı
                        // varyant gider, böylece A/B ölçümü karışmaz.
                        lead.mesaj_varyanti = Some(self.defter.varyant(&anahtar));
                    }
                    self.defter.kaydet();
                    if self.kaynak == Kaynak::Google {
                        incelemeye_gore_sirala(&mut bulunanlar);
                    } else {
                        leads_sirala(&mut bulunanlar);
                    }
                    let ozet = self.defter.ozet();
                    self.logs.push(format!(
                        "[i] Müşteri defteri: {} yeni kayıt, {bilinen} bilinen işletme · toplam {} kayıt ({} kara listede).",
                        adet.saturating_sub(bilinen),
                        ozet.toplam,
                        ozet.kara_liste
                    ));
                    self.leads = bulunanlar;
                    let il = self.cities[self.selected_city_idx].0;
                    if il_geneline_gecilir(adet, self.il_geneli) {
                        // İlçede kayıt yok: kullanıcı beklemeden tüm ilçeler taranır.
                        let ilce = self.cities[self.selected_city_idx]
                            .1
                            .get(self.selected_district_idx)
                            .copied()
                            .unwrap_or("");
                        self.logs.push(format!(
                            "[i] {ilce} / {il} taramasında kayıt bulunamadı; il geneli taraması otomatik başlatıldı."
                        ));
                        self.il_geneli = true;
                        self.bos_ilce_uyari = None;
                        self.tarama_baslat();
                    } else if adet == 0 {
                        self.logs.push(format!(
                            "[i] {il} genelinde de web sitesi olmayan işletme bulunamadı; başka bir sektör deneyin."
                        ));
                        self.bos_ilce_uyari = Some(format!("{il} geneli"));
                    } else {
                        self.bos_ilce_uyari = None;
                        self.logs
                            .push(format!("[✓] {adet} işletme listeye eklendi."));
                    }
                }
                Mesaj::TaramaBasarisiz => {
                    self.is_running = false;
                    self.ilerleme = None;
                }
            }
        }

        // Tarama sürerken günlük ve ilerleme çubuğu kendiliğinden güncellensin;
        // eframe yalnızca olaylarda çizdiği için düzenli yeniden çizim istenir.
        if self.is_running {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        let secili = self.leads.iter().filter(|lead| lead.selected).count();
        let demo_hazir = self
            .leads
            .iter()
            .filter(|lead| lead.demo_yolu.is_some())
            .count();
        let gonderen = whatsapp_numarasi(&self.whatsapp_phone)
            .map(|numara| format!("+{numara}"))
            .unwrap_or_default();

        let il = self.cities[self.selected_city_idx].0;
        let ilce = self.cities[self.selected_city_idx].1[self.selected_district_idx];
        let sektor = self.sectors[self.selected_sector_idx];

        // --- Üst bar ---------------------------------------------------------
        egui::TopBottomPanel::top("ust_bar")
            .exact_height(72.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(22.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (kutu, _) =
                        ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(kutu, egui::Rounding::same(10.0), VURGU);
                    p.text(
                        kutu.center(),
                        egui::Align2::CENTER_CENTER,
                        "N",
                        baslik_font(17.0),
                        KOYU_YAZI,
                    );

                    ui.add_space(11.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Nexus Enterprise")
                                .font(baslik_font(15.0))
                                .color(METIN),
                        );
                        ui.label(
                            egui::RichText::new("İşletme keşif ve demo otomasyonu")
                                .size(10.5)
                                .color(DIM),
                        );
                    });

                    ui.add_space(20.0);
                    let (cizgi, _) =
                        ui.allocate_exact_size(egui::vec2(1.0, 30.0), egui::Sense::hover());
                    ui.painter().rect_filled(cizgi, 0.0, KENAR);

                    ui.add_space(20.0);
                    kontur_pill(
                        ui,
                        &if self.il_geneli {
                            format!("{il} · tüm ilçeler")
                        } else {
                            format!("{il} · {ilce}")
                        },
                    );
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(sektor).size(11.0).color(DIM));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        durum_pill(ui, self.is_running);
                        ui.add_space(10.0);
                        if secili > 0 {
                            ui.label(
                                egui::RichText::new(format!("{secili} seçili"))
                                    .size(11.5)
                                    .color(VURGU),
                            );
                            ui.add_space(8.0);
                        }
                        ui.label(
                            egui::RichText::new(format!("{} sonuç", self.leads.len()))
                                .size(11.5)
                                .color(SOLUK),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("canlı OSM verisi")
                                .size(10.5)
                                .color(DIM),
                        );
                        ui.add_space(6.0);
                        let (nokta, _) =
                            ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                        ui.painter().circle_filled(nokta.center(), 3.0, YESIL);
                    });
                });

                let alt = ui.max_rect().bottom() - 0.5;
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    alt,
                    egui::Stroke::new(1.0_f32, KENAR),
                );
            });

        // --- Sol panel: filtreler -------------------------------------------
        egui::SidePanel::left("filtre_paneli")
            .resizable(false)
            .exact_width(330.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(18.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    bolum_etiketi(ui, "FİLTRELER");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hayalet_buton(ui, "Sıfırla").clicked() {
                            self.selected_sector_idx = 0;
                            self.selected_district_idx = 0;
                            self.il_geneli = false;
                            self.bos_ilce_uyari = None;
                            self.city_search.clear();
                            self.district_search.clear();
                            self.sector_search.clear();
                        }
                    });
                });
                ui.label(
                    egui::RichText::new(format!(
                        "{} il · {} ilçe · {} sektör",
                        iller::ILLER.len(),
                        iller::ILCELER,
                        self.sectors.len()
                    ))
                    .size(10.5)
                    .color(DIM),
                );
                ui.add_space(10.0);

                let wa_yuksekligi = if self.whatsapp_acik { 200.0 } else { 72.0 };
                let kaydirma_yuksekligi = (ui.available_height() - wa_yuksekligi - 78.0).max(160.0);
                egui::ScrollArea::vertical()
                    .max_height(kaydirma_yuksekligi)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // İl
                        filtre_basligi(ui, "İl", il);
                        arama_kutusu(ui, &mut self.city_search, "İl ara");
                        ui.add_space(5.0);
                        let il_onceki = self.selected_city_idx;
                        {
                            let sorgu = arama_anahtari(&self.city_search);
                            let cities = &self.cities;
                            let secili_idx = &mut self.selected_city_idx;
                            secim_listesi(ui, "il_listesi", 104.0, None, |ui| {
                                for (idx, (ad, _)) in cities.iter().enumerate() {
                                    if !sorgu.is_empty() && !arama_anahtari(ad).contains(&sorgu) {
                                        continue;
                                    }
                                    if liste_satiri(ui, *secili_idx == idx, ad).clicked() {
                                        *secili_idx = idx;
                                    }
                                }
                            });
                        }
                        if il_onceki != self.selected_city_idx {
                            self.selected_district_idx = 0;
                            self.bos_ilce_uyari = None;
                            self.district_search.clear();
                            self.ilce_kaydir = true;
                        }
                        ui.add_space(12.0);

                        // İlçe
                        if self.selected_district_idx >= self.cities[self.selected_city_idx].1.len()
                        {
                            self.selected_district_idx = 0;
                        }
                        let ilce =
                            self.cities[self.selected_city_idx].1[self.selected_district_idx];
                        if filtre_basligi_ilce(ui, "İlçe", ilce, self.il_geneli) {
                            self.il_geneli = !self.il_geneli;
                            self.bos_ilce_uyari = None;
                        }
                        arama_kutusu(ui, &mut self.district_search, "İlçe ara");
                        ui.add_space(5.0);
                        {
                            let sorgu = arama_anahtari(&self.district_search);
                            let ofset = if self.ilce_kaydir && !self.il_geneli {
                                Some((self.selected_district_idx as f32 * 26.0 - 26.0).max(0.0))
                            } else {
                                None
                            };
                            let ilceler = &self.cities[self.selected_city_idx].1;
                            let il_geneli = self.il_geneli;
                            let mut secili_idx = self.selected_district_idx;
                            secim_listesi(ui, "ilce_listesi", 130.0, ofset, |ui| {
                                for (idx, ad) in ilceler.iter().enumerate() {
                                    if !sorgu.is_empty() && !arama_anahtari(ad).contains(&sorgu) {
                                        continue;
                                    }
                                    if liste_satiri(ui, !il_geneli && secili_idx == idx, ad)
                                        .clicked()
                                    {
                                        secili_idx = idx;
                                    }
                                }
                            });
                            if secili_idx != self.selected_district_idx {
                                self.selected_district_idx = secili_idx;
                                self.il_geneli = false;
                                self.bos_ilce_uyari = None;
                            }
                        }
                        self.ilce_kaydir = false;
                        ui.add_space(12.0);

                        // Sektör: seçim iki sütunlu panelde yapılır
                        filtre_basligi(ui, "Sektör", sektor);
                        if sektor_secici(ui, sektor).clicked() {
                            self.sektor_panel_acik = true;
                            self.sector_search.clear();
                            let grup = sektor_grubu(sektor);
                            self.sektor_panel_grup = sektor_gruplari()
                                .iter()
                                .position(|ad| *ad == grup)
                                .unwrap_or(0);
                        }

                        // Kaynak: OSM / Google / Hibrit.
                        ui.add_space(12.0);
                        filtre_basligi(ui, "Kaynak", self.kaynak.etiket());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for kaynak in Kaynak::HEPSI {
                                if filtre_cipi(ui, kaynak.etiket(), self.kaynak == kaynak).clicked()
                                {
                                    self.kaynak = kaynak;
                                }
                            }
                        });
                    });

                ui.add_space(10.0);
                self.whatsapp_ayarlari(ui);
                ui.add_space(8.0);
                let tara_genislik = ui.available_width();
                let tara = birincil_buton(
                    ui,
                    if self.is_running {
                        "Taranıyor..."
                    } else if self.il_geneli {
                        "İl genelini tara"
                    } else {
                        "İşletmeleri tara"
                    },
                    !self.is_running,
                    Some(egui::vec2(tara_genislik, 42.0)),
                );
                if tara.clicked() {
                    self.tarama_baslat();
                }
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(if self.il_geneli {
                            "Tüm ilçeler sırayla · en fazla 20 kayıt"
                        } else {
                            "Sorgu başına en fazla 20 kayıt"
                        })
                        .size(10.0)
                        .color(DIM),
                    );
                });
            });

        // --- Alt panel: işlem günlüğü ---------------------------------------
        egui::TopBottomPanel::bottom("gunluk_paneli")
            .resizable(true)
            .default_height(100.0)
            .min_height(84.0)
            .max_height(280.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(22.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (nokta, _) =
                        ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                    ui.painter().circle_filled(nokta.center(), 3.0, DIM);
                    bolum_etiketi(ui, "İŞLEM GÜNLÜĞÜ");
                    ui.label(
                        egui::RichText::new(format!("{} satır", self.logs.len()))
                            .size(10.0)
                            .color(DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hayalet_buton(ui, "Temizle").clicked() {
                            self.logs.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                let gunluk_yuksekligi = (ui.available_height() - 18.0).max(40.0);
                egui::Frame::none()
                    .fill(GIRINTI)
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(gunluk_yuksekligi)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.logs.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Henüz işlem yok.")
                                            .size(11.0)
                                            .color(DIM),
                                    );
                                }
                                for log in &self.logs {
                                    ui.label(
                                        egui::RichText::new(log)
                                            .font(kod_font(11.0))
                                            .color(log_rengi(log)),
                                    );
                                }
                            });
                    });
            });

        // --- Sağ panel: işletme detayı ---------------------------------------
        self.detay_paneli(ctx, &gonderen);
        self.sektor_paneli(ctx);
        self.takip_paneli(ctx);
        self.onizleme_paneli(ctx, &gonderen);

        // --- Orta panel: sonuçlar --------------------------------------------
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ARKA_PLAN)
                    .inner_margin(egui::Margin::symmetric(22.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let kart_genisligi = ((ui.available_width() - 20.0) / 3.0).max(120.0);
                    istatistik_karti(
                        ui,
                        "BULUNAN",
                        &self.leads.len().to_string(),
                        MAVI,
                        kart_genisligi,
                    );
                    ui.add_space(10.0);
                    istatistik_karti(ui, "SEÇİLİ", &secili.to_string(), VURGU, kart_genisligi);
                    ui.add_space(10.0);
                    istatistik_karti(
                        ui,
                        "DEMO HAZIR",
                        &demo_hazir.to_string(),
                        YESIL,
                        kart_genisligi,
                    );
                });
                ui.add_space(14.0);

                // Araç çubuğu: sonuç araması solda, işlemler sağda.
                let bugun = bugun_anahtari();
                let takip = self.takip_bekleyenler(&bugun);
                let yeni_sayisi = self
                    .leads
                    .iter()
                    .filter(|lead| lead.durum == musteriler::Durum::Yeni)
                    .count();
                ui.horizontal(|ui| {
                    let arama_alani = 250.0_f32.min(ui.available_width() * 0.4);
                    ui.allocate_ui_with_layout(
                        egui::vec2(arama_alani, 34.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            arama_kutusu(ui, &mut self.sonuc_arama, "Sonuçlarda ara");
                        },
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let demo_metin = if secili > 0 {
                            format!("Seçilenlere demo üret ({secili})")
                        } else {
                            "Seçilenlere demo üret".to_string()
                        };
                        let demo_butonu = ikincil_buton(
                            ui,
                            &demo_metin,
                            secili > 0,
                            Some(egui::vec2(200.0, 32.0)),
                        );
                        if demo_butonu.clicked() {
                            let mut uretilen = 0usize;
                            let mut hatali = 0usize;
                            for lead in self.leads.iter_mut() {
                                if !lead.selected || lead.demo_yolu.is_some() {
                                    continue;
                                }
                                match demo_uret(lead, &gonderen) {
                                    Ok(yol) => {
                                        lead.demo_yolu = Some(yol);
                                        uretilen += 1;
                                    }
                                    Err(_) => hatali += 1,
                                }
                            }
                            self.logs.push(if hatali == 0 {
                                format!("[✓] {uretilen} demo hazırlandı.")
                            } else {
                                format!(
                                    "[✓] {uretilen} demo hazırlandı, {hatali} kayıt üretilemedi."
                                )
                            });
                        }
                        ui.add_space(6.0);
                        if hayalet_buton(ui, "CSV indir").clicked() {
                            match self.defter.csv_kaydet() {
                                Ok(yol) => self.logs.push(format!(
                                    "[✓] {} müşteri {yol} dosyasına aktarıldı.",
                                    self.defter.ozet().toplam
                                )),
                                Err(hata) => {
                                    self.logs.push(format!("[!] CSV yazılamadı: {hata}"));
                                }
                            }
                        }
                        ui.add_space(6.0);
                        if hayalet_buton(ui, "Tümünü seç").clicked() {
                            for lead in self.leads.iter_mut() {
                                lead.selected = true;
                            }
                        }
                        ui.add_space(6.0);
                        if hayalet_buton(ui, "Seçimi temizle").clicked() {
                            for lead in self.leads.iter_mut() {
                                lead.selected = false;
                            }
                        }
                        ui.add_space(6.0);
                        if hayalet_buton(ui, "Demos klasörünü aç").clicked() {
                            let _ = fs::create_dir_all(DEMO_KLASORU);
                            if let Err(hata) = klasoru_ac(DEMO_KLASORU) {
                                self.logs
                                    .push(format!("[!] Demos klasörü açılamadı: {hata}"));
                            }
                        }
                    });
                });

                // Günlük çalışma listesi: tümü / yalnızca yeni / takip bekleyen.
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(egui::RichText::new("LİSTE").size(9.5).color(DIM));
                    ui.add_space(2.0);
                    for (suzgec, etiket) in [
                        (ListeSuzgeci::Hepsi, format!("Tümü ({})", self.leads.len())),
                        (ListeSuzgeci::Yeni, format!("Yeni ({yeni_sayisi})")),
                        (ListeSuzgeci::Takip, format!("Takip ({})", takip.len())),
                    ] {
                        if filtre_cipi(ui, &etiket, self.liste_suzgeci == suzgec).clicked() {
                            self.liste_suzgeci = suzgec;
                        }
                    }
                    // Sağda: bugünkü gönderim hakkı ve takip kuyruğu.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let kuyruk = self.defter.takip_edilecekler(&bugun, TAKIP_GUN).len();
                        if hayalet_buton(ui, &format!("Takip kuyruğu ({kuyruk})")).clicked() {
                            self.takip_panel_acik = true;
                        }
                        ui.add_space(10.0);
                        let gunluk = self.defter.gun_gonderim(&bugun);
                        ui.label(
                            egui::RichText::new(format!(
                                "bugün {gunluk}/{GUNLUK_GONDERIM_LIMITI} mesaj"
                            ))
                            .size(10.5)
                            .color(
                                if gunluk >= GUNLUK_GONDERIM_LIMITI {
                                    VURGU
                                } else {
                                    DIM
                                },
                            ),
                        );
                    });
                });
                ui.add_space(10.0);

                if let Some((tamamlanan, toplam)) = self.ilerleme.filter(|_| self.is_running) {
                    ilerleme_cubugu(ui, tamamlanan, toplam);
                    ui.add_space(10.0);
                }

                // Sonuç listesi
                let liste_yuksekligi = ui.available_height();
                let taranıyor = self.is_running;
                let mut il_geneli_baslat = false;
                let bos_ilce = self.bos_ilce_uyari.clone();
                let il_geneli_simdi = self.il_geneli;
                let mut yeni_detay = self.detay_idx;
                let mut onizleme_idx: Option<usize> = None;
                let limit_doldu = self.gunluk_limit_doldu();
                {
                    let arama = arama_anahtari(&self.sonuc_arama);
                    let suzgec = self.liste_suzgeci;
                    let acik_detay = self.detay_idx;
                    let leads = &mut self.leads;
                    let logs = &mut self.logs;
                    egui::ScrollArea::vertical()
                        .id_source("sonuc_listesi")
                        .auto_shrink([false, false])
                        .max_height(liste_yuksekligi)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if leads.is_empty() {
                                let uyari = bos_ilce.as_deref().map(|ilce| (ilce, sektor));
                                bos_durum(
                                    ui,
                                    taranıyor,
                                    uyari,
                                    il_geneli_simdi,
                                    &mut il_geneli_baslat,
                                );
                                return;
                            }
                            let mut gosterilen = 0usize;
                            for (idx, lead) in leads.iter_mut().enumerate() {
                                if !arama.is_empty() {
                                    let metin =
                                        format!("{} {} {}", lead.name, lead.address, lead.category);
                                    if !arama_anahtari(&metin).contains(&arama) {
                                        continue;
                                    }
                                }
                                let suzuldu = match suzgec {
                                    ListeSuzgeci::Hepsi => false,
                                    ListeSuzgeci::Yeni => lead.durum != musteriler::Durum::Yeni,
                                    ListeSuzgeci::Takip => {
                                        let anahtar = musteriler::anahtar_uret(
                                            &lead.kaynak,
                                            &lead.name,
                                            lead.konum,
                                        );
                                        !takip.contains(&anahtar)
                                    }
                                };
                                if suzuldu {
                                    continue;
                                }
                                gosterilen += 1;
                                let (detay_iste, wa_iste) = kart_ciz(
                                    ui,
                                    lead,
                                    &gonderen,
                                    logs,
                                    acik_detay == Some(idx),
                                    limit_doldu,
                                );
                                if detay_iste {
                                    yeni_detay = if acik_detay == Some(idx) {
                                        None
                                    } else {
                                        Some(idx)
                                    };
                                }
                                if wa_iste {
                                    onizleme_idx = Some(idx);
                                }
                                ui.add_space(8.0);
                            }
                            if gosterilen == 0 {
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new("Bu filtreyle eşleşen sonuç yok.")
                                        .size(11.5)
                                        .color(DIM),
                                );
                            }
                        });
                }
                if yeni_detay != self.detay_idx {
                    self.detay_idx = yeni_detay;
                }
                if let Some(idx) = onizleme_idx {
                    self.gonderime_hazirla(idx, &gonderen);
                }
                if il_geneli_baslat {
                    self.il_geneli = true;
                    self.tarama_baslat();
                }
            });
    }
}

fn adim_karti(ui: &mut egui::Ui, no: &str, baslik: &str, aciklama: &str, genislik: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(genislik, 64.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(12.0), KART);
    p.rect_stroke(
        rect,
        egui::Rounding::same(12.0),
        egui::Stroke::new(1.0_f32, KENAR),
    );
    p.text(
        rect.left_top() + egui::vec2(14.0, 15.0),
        egui::Align2::LEFT_TOP,
        no,
        baslik_font(11.0),
        VURGU.gamma_multiply(0.85),
    );
    p.text(
        rect.left_top() + egui::vec2(40.0, 13.0),
        egui::Align2::LEFT_TOP,
        baslik,
        baslik_font(12.0),
        METIN,
    );
    p.text(
        rect.left_top() + egui::vec2(40.0, 33.0),
        egui::Align2::LEFT_TOP,
        aciklama,
        govde_font(10.5),
        DIM,
    );
}

/// İlerleme oranı: toplam sıfırsa 0, aksi halde 0..=1 aralığına kırpılır.
fn ilerleme_orani(tamamlanan: usize, toplam: usize) -> f32 {
    if toplam == 0 {
        return 0.0;
    }
    (tamamlanan as f32 / toplam as f32).clamp(0.0, 1.0)
}

/// İl geneli taramasının ilçe ilerlemesi: ince çubuk ve "12/39" sayacı.
fn ilerleme_cubugu(ui: &mut egui::Ui, tamamlanan: usize, toplam: usize) {
    let oran = ilerleme_orani(tamamlanan, toplam);
    let genislik = ui.available_width();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "İl geneli tarama · {tamamlanan}/{} ilçe",
                toplam.max(1)
            ))
            .size(11.0)
            .color(SOLUK),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("%{}", (oran * 100.0).round() as i32))
                    .size(11.0)
                    .color(VURGU),
            );
        });
    });
    ui.add_space(5.0);

    let (rect, _) = ui.allocate_exact_size(egui::vec2(genislik, 6.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(3.0), GIRINTI);
    if oran > 0.0 {
        let dolu = egui::Rect::from_min_size(
            rect.min,
            egui::vec2((rect.width() * oran).max(6.0), rect.height()),
        );
        p.rect_filled(dolu, egui::Rounding::same(3.0), VURGU);
    }
}

/// Boş sonuç alanı: büyük ikon, kısa yönlendirme ve üç adımlı ipucu.
fn bos_durum(
    ui: &mut egui::Ui,
    taranıyor: bool,
    uyari: Option<(&str, &str)>,
    il_geneli: bool,
    il_geneli_baslat: &mut bool,
) {
    ui.add_space(46.0);
    ui.vertical_centered(|ui| {
        let (daire, _) = ui.allocate_exact_size(egui::vec2(76.0, 76.0), egui::Sense::hover());
        let merkez = daire.center();
        let p = ui.painter();
        p.circle_filled(merkez, 38.0, KART);
        p.circle_stroke(merkez, 37.5, egui::Stroke::new(1.0_f32, KENAR));
        p.circle_stroke(
            merkez + egui::vec2(-3.0, -3.0),
            11.0,
            egui::Stroke::new(2.0_f32, SOLUK),
        );
        p.line_segment(
            [
                merkez + egui::vec2(5.0, 5.0),
                merkez + egui::vec2(13.0, 13.0),
            ],
            egui::Stroke::new(2.0_f32, SOLUK),
        );

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(if taranıyor {
                "İşletmeler taranıyor"
            } else if uyari.is_some() && il_geneli {
                "İl genelinde kayıt çıkmadı"
            } else if uyari.is_some() {
                "Bu ilçede kayıt çıkmadı"
            } else {
                "Taramaya hazır"
            })
            .font(baslik_font(17.0))
            .color(METIN),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(if taranıyor {
                "OpenStreetMap kayıtları çekiliyor, sonuçlar birazdan listelenecek."
            } else if uyari.is_some() && il_geneli {
                "Tüm ilçeler tarandı; bu sektörde web sitesi olmayan işletme kaydı yok.\nBaşka bir sektör seçmek daha verimli olur."
            } else if uyari.is_some() {
                "Kayıt az: bu sektör ilçede tek tük kayıtlı olabilir.\nİl geneli taraması tüm ilçeleri sırayla gezer."
            } else {
                "Soldan il, ilçe ve sektör seçip “İşletmeleri tara” butonuna basın.\nListede yalnızca web sitesi olmayan gerçek işletmeler görünür."
            })
            .size(11.5)
            .color(SOLUK),
        );

        if let Some((ilce, sektor)) = uyari.filter(|_| !taranıyor) {
            ui.add_space(16.0);
            let kart_genislik = 470.0_f32.min(ui.available_width() - 24.0);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - kart_genislik) / 2.0).max(0.0));
                egui::Frame::none()
                    .fill(VURGU.gamma_multiply(0.09))
                    .stroke(egui::Stroke::new(1.0_f32, VURGU.gamma_multiply(0.45)))
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                    .show(ui, |ui| {
                        ui.set_width(kart_genislik - 32.0);
                        ui.label(
                            egui::RichText::new(format!("{ilce} · {sektor}"))
                                .font(baslik_font(12.5))
                                .color(VURGU),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(if il_geneli {
                                "İl geneli taraması otomatik yapıldı ve sonuç boş döndü. Sektör OpenStreetMap'te çok seyrek olabilir."
                            } else {
                                "Bu ilçede web sitesi olmayan kayıt dönmedi. Sektör burada seyrek olabilir; il genelinde tarayınca komşu ilçeler de taranır."
                            })
                            .size(11.0)
                            .color(SOLUK),
                        );
                        if !il_geneli {
                            ui.add_space(9.0);
                            if ikincil_buton(
                                ui,
                                "İl genelinde tara",
                                true,
                                Some(egui::vec2(168.0, 30.0)),
                            )
                            .clicked()
                            {
                                *il_geneli_baslat = true;
                            }
                        }
                    });
            });
        }

        ui.add_space(20.0);
        let adim_genisligi = 196.0;
        let aralik = 12.0;
        ui.horizontal(|ui| {
            ui.add_space(
                ((ui.available_width() - (adim_genisligi * 3.0 + aralik * 2.0)) / 2.0).max(0.0),
            );
            adim_karti(
                ui,
                "01",
                "Konum seç",
                "İl ve ilçe sınırı",
                adim_genisligi,
            );
            ui.add_space(aralik);
            adim_karti(
                ui,
                "02",
                "Sektör seç",
                "Doğru işletme türü",
                adim_genisligi,
            );
            ui.add_space(aralik);
            adim_karti(
                ui,
                "03",
                "Tara ve demo üret",
                "Tek tıkla taslak site",
                adim_genisligi,
            );
        });
    });
}

/// Başlık satırının sağındaki, tıklanabilir seçim çipi.
fn filtre_cipi(ui: &mut egui::Ui, metin: &str, secili: bool) -> egui::Response {
    let boy = egui::vec2(metin_genisligi(ui, metin, 10.5) + 22.0, 22.0);
    let (rect, yanit) = ui.allocate_exact_size(boy, egui::Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(yanit.id, yanit.hovered(), 0.10);
    let (dolgu, kenar, yazi) = if secili {
        (
            VURGU.gamma_multiply(0.15),
            VURGU.gamma_multiply(0.50),
            VURGU,
        )
    } else {
        (
            KART,
            karistir(KENAR, KENAR_GUCLU, t),
            karistir(SOLUK, METIN, t),
        )
    };
    let p = ui.painter();
    p.rect_filled(rect, egui::Rounding::same(99.0), dolgu);
    p.rect_stroke(
        rect,
        egui::Rounding::same(99.0),
        egui::Stroke::new(1.0_f32, kenar),
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        metin,
        govde_font(10.5),
        yazi,
    );
    if yanit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    yanit
}

/// İlçe başlığı: sağda seçili ilçe ve "Tüm ilçeler" çipi. Çipe tıklanınca
/// `true` döner (il geneli aç/kapa).
fn filtre_basligi_ilce(ui: &mut egui::Ui, baslik: &str, deger: &str, il_geneli: bool) -> bool {
    let mut tiklanti = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(baslik).size(11.0).color(SOLUK));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if filtre_cipi(ui, "Tüm ilçeler", il_geneli).clicked() {
                tiklanti = true;
            }
            if !il_geneli {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(deger)
                        .font(baslik_font(11.0))
                        .color(VURGU),
                );
            }
        });
    });
    ui.add_space(5.0);
    tiklanti
}

fn filtre_basligi(ui: &mut egui::Ui, baslik: &str, deger: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(baslik).size(11.0).color(SOLUK));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(deger)
                    .font(baslik_font(11.0))
                    .color(VURGU),
            );
        });
    });
    ui.add_space(5.0);
}

fn secim_listesi(
    ui: &mut egui::Ui,
    kimlik: &str,
    yukseklik: f32,
    kaydirma_ofseti: Option<f32>,
    icerik: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::none()
        .fill(GIRINTI)
        .stroke(egui::Stroke::new(1.0_f32, KENAR))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(4.0, 4.0))
        .show(ui, |ui| {
            let mut alan = egui::ScrollArea::vertical()
                .id_source(kimlik)
                .max_height(yukseklik)
                .auto_shrink([false, false]);
            if let Some(ofset) = kaydirma_ofseti {
                alan = alan.vertical_scroll_offset(ofset);
            }
            alan.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 1.0;
                icerik(ui);
            });
        });
}
// ---------------------------------------------------------------------------
// Gerçek veri kaynağı: OpenStreetMap (Nominatim + Overpass API)
//
// İlçe sınırı Nominatim ile koordinata çevrilir, işletmeler Overpass API'den
// gerçek OSM kayıtları olarak çekilir. Web sitesi etiketi (website/contact:
// website/url/contact:url) bulunmayan işletmeler listelenir.
// ---------------------------------------------------------------------------

const NOMINATIM_URL: &str = "https://nominatim.openstreetmap.org/search";

/// Overpass sunucuları sırayla denenir. Üçü de dünya çapında veri sunar.
/// Bölgesel aynalar (ör. overpass.osm.ch yalnızca İsviçre verisi tutar)
/// bilerek listelenmez: kapsam dışı bölgede hata değil **boş** dönerler, bu da
/// "bu ilçede kayıt yok" gibi yanlış bir sonuç üretir.
const OVERPASS_SUNUCULARI: [(&str, &str); 3] = [
    ("https://overpass-api.de/api/interpreter", "overpass-api.de"),
    (
        "https://z.overpass-api.de/api/interpreter",
        "z.overpass-api.de",
    ),
    (
        "https://overpass.private.coffee/api/interpreter",
        "private.coffee",
    ),
];

/// Bir ilçenin Overpass istekleri için toplam süre bütçesi. Aşılırsa kalan
/// sunucular denenmeden hata dönülür; böylece yoğun saatte il geneli tarama
/// tek bir ilçede kilitlenmez.
const OVERPASS_BUTCE: Duration = Duration::from_secs(150);

/// Bir işletmenin gerçekten web sitesi olup olmadığını gösteren OSM etiketleri.
const WEB_SITESI_ETIKETLERI: [&str; 4] = ["website", "contact:website", "url", "contact:url"];

struct Konum {
    enlem: f64,
    boylam: f64,
    yaricap_m: u32,
    /// Nominatim'in döndürdüğü idari sınır relation/way kaydından türetilen
    /// Overpass alan kimliği; varsa tarama ilçe poligonu içinde yapılır.
    alan_id: Option<u64>,
}

/// Önbellekte saklanan konum kaydı.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct KonumKaydi {
    enlem: f64,
    boylam: f64,
    yaricap_m: u32,
    alan_id: Option<u64>,
    /// Önbelleğe alındığı tarih (YYYY-MM-DD).
    tarih: String,
}

const KONUM_ONBELLEK_DOSYASI: &str = "konum_onbellek.json";
/// Konumun kaç gün geçerli sayılacağı; ilçe sınırları yıllarca değişmez.
const KONUM_TTL_GUN: i64 = 180;

/// İlçe → konum önbelleği. İl geneli taramada aynı ilçeler tekrar tekrar
/// çözümlenmesin diye `konum_onbellek.json` dosyasında tutulur.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct KonumOnbellek {
    #[serde(default)]
    kayitlar: HashMap<String, KonumKaydi>,
}

impl KonumOnbellek {
    fn yukle() -> Self {
        fs::read_to_string(KONUM_ONBELLEK_DOSYASI)
            .ok()
            .and_then(|metin| serde_json::from_str(&metin).ok())
            .unwrap_or_default()
    }

    fn kaydet(&self) {
        if let Ok(metin) = serde_json::to_string_pretty(self) {
            let _ = fs::write(KONUM_ONBELLEK_DOSYASI, metin);
        }
    }

    fn anahtar(il: &str, ilce: &str) -> String {
        format!("{}|{}", arama_anahtari(il), arama_anahtari(ilce))
    }

    /// Önbellekteki konum; `KONUM_TTL_GUN` günden eskiyse yok sayılır.
    fn al(&self, il: &str, ilce: &str) -> Option<Konum> {
        let kayit = self.kayitlar.get(&Self::anahtar(il, ilce))?;
        let gun = gun_farki(&kayit.tarih, &bugun_anahtari())?;
        if gun > KONUM_TTL_GUN {
            return None;
        }
        Some(Konum {
            enlem: kayit.enlem,
            boylam: kayit.boylam,
            yaricap_m: kayit.yaricap_m,
            alan_id: kayit.alan_id,
        })
    }

    fn ekle(&mut self, il: &str, ilce: &str, konum: &Konum) {
        self.kayitlar.insert(
            Self::anahtar(il, ilce),
            KonumKaydi {
                enlem: konum.enlem,
                boylam: konum.boylam,
                yaricap_m: konum.yaricap_m,
                alan_id: konum.alan_id,
                tarih: bugun_anahtari(),
            },
        );
    }
}

const SONUC_ONBELLEK_DOSYASI: &str = "sonuc_onbellek.json";

/// Sonuç önbelleğinin varsayılan geçerlilik süresi (gün). `SONUC_TTL_GUN=0`
/// önbelleği tamamen kapatır, böylece her tarama canlı veri getirir.
const SONUC_TTL_GUN: i64 = 7;

fn sonuc_ttl_gun() -> i64 {
    std::env::var("SONUC_TTL_GUN")
        .ok()
        .and_then(|deger| deger.trim().parse::<i64>().ok())
        .unwrap_or(SONUC_TTL_GUN)
        .max(0)
}

/// Bir ilçe + sektör için saklanan arama sonucu.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SonucKaydi {
    tarih: String,
    siteli: usize,
    leads: Vec<BusinessLead>,
    /// Sonucu veren Overpass sunucusu (bilgi amaçlı).
    #[serde(default)]
    sunucu: String,
}

/// Önbellekten dönen kayıt. `gun` = kaç gün önce çekildiği.
struct SonucOnbellekKaydi {
    leads: Vec<BusinessLead>,
    siteli: usize,
    gun: i64,
}

/// İlçe + sektör başına Overpass sonuçları. Aynı ilçe kısa süre içinde tekrar
/// tarandığında Overpass'a hiç gidilmez; il geneli taramada en büyük zaman
/// kaybı ilçe başına atılan ağır sorgulardır.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct SonucOnbellek {
    #[serde(default)]
    kayitlar: HashMap<String, SonucKaydi>,
}

impl SonucOnbellek {
    fn yukle() -> Self {
        fs::read_to_string(SONUC_ONBELLEK_DOSYASI)
            .ok()
            .and_then(|metin| serde_json::from_str(&metin).ok())
            .unwrap_or_default()
    }

    fn kaydet(&self) {
        if let Ok(metin) = serde_json::to_string(self) {
            let _ = fs::write(SONUC_ONBELLEK_DOSYASI, metin);
        }
    }

    fn anahtar(il: &str, ilce: &str, sektor: &str) -> String {
        format!(
            "{}|{}|{}",
            arama_anahtari(il),
            arama_anahtari(ilce),
            arama_anahtari(sektor)
        )
    }

    /// TTL içindeki kayıt; yoksa veya eskimişse `None`.
    fn al(&self, il: &str, ilce: &str, sektor: &str) -> Option<SonucOnbellekKaydi> {
        let ttl = sonuc_ttl_gun();
        if ttl == 0 {
            return None;
        }
        let kayit = self.kayitlar.get(&Self::anahtar(il, ilce, sektor))?;
        let gun = gun_farki(&kayit.tarih, &bugun_anahtari())?;
        if gun < 0 || gun > ttl {
            return None;
        }
        Some(SonucOnbellekKaydi {
            leads: kayit.leads.clone(),
            siteli: kayit.siteli,
            gun,
        })
    }

    fn ekle(
        &mut self,
        il: &str,
        ilce: &str,
        sektor: &str,
        leads: &[BusinessLead],
        siteli: usize,
        sunucu: &str,
    ) {
        self.kayitlar.insert(
            Self::anahtar(il, ilce, sektor),
            SonucKaydi {
                tarih: bugun_anahtari(),
                siteli,
                leads: leads.to_vec(),
                sunucu: sunucu.to_string(),
            },
        );
    }
}

/// İlçe taramasının sonucu: önbellekten mi, ağdan mı geldiği günlükte
/// birbirinden ayrılır.
enum IlceTaramasi {
    Onbellek(SonucOnbellekKaydi),
    Ag {
        leads: Vec<BusinessLead>,
        siteli: usize,
        sunucu: &'static str,
    },
    Hata(String),
}

/// `IlceTaramasi::parcala` sonucu: liste, sitesi olan kayıt sayısı, yanıtı
/// veren sunucu ve önbellek günü.
type TaramaParcasi = (Vec<BusinessLead>, usize, &'static str, Option<i64>);

impl IlceTaramasi {
    fn hata(&self) -> Option<&str> {
        match self {
            IlceTaramasi::Hata(hata) => Some(hata),
            _ => None,
        }
    }

    /// `(liste, siteli, sunucu, önbellek_gunu)`; önbellek günü `None` ise sonuç
    /// canlı ağdan geldi.
    fn parcala(self) -> Result<TaramaParcasi, String> {
        match self {
            IlceTaramasi::Onbellek(kayit) => Ok((
                kayit.leads,
                kayit.siteli,
                OVERPASS_SUNUCULARI[0].1,
                Some(kayit.gun),
            )),
            IlceTaramasi::Ag {
                leads,
                siteli,
                sunucu,
            } => Ok((leads, siteli, sunucu, None)),
            IlceTaramasi::Hata(hata) => Err(hata),
        }
    }
}

/// Bir ilçeyi tarar: önce sonuç önbelleğine bakar, yoksa Overpass'a gider ve
/// gelen sonucu önbelleğe yazar.
fn ilce_taramasi(
    ajan: &ureq::Agent,
    onbellek: &mut SonucOnbellek,
    il: &str,
    ilce: &str,
    sektor: &str,
    konum: &Konum,
) -> IlceTaramasi {
    if let Some(kayit) = onbellek.al(il, ilce, sektor) {
        return IlceTaramasi::Onbellek(kayit);
    }
    match sitesiz_isletmeler(ajan, il, ilce, sektor, konum) {
        Ok((leads, siteli, sunucu)) => {
            onbellek.ekle(il, ilce, sektor, &leads, siteli, sunucu);
            onbellek.kaydet();
            IlceTaramasi::Ag {
                leads,
                siteli,
                sunucu,
            }
        }
        Err(hata) => IlceTaramasi::Hata(hata),
    }
}

fn http_ajani() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent("NexusEnterprise/0.1 (Windows masaustu isletme kesif uygulamasi)")
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .build()
}

/// İstek başına en fazla deneme sayısı (Google, Nominatim).
const ISTEK_DENEMESI: usize = 3;

/// Overpass'ta sunucu başına deneme sayısı. Sunucu zinciri zaten yedek
/// sağladığı için burada daha az denenir; böylece tek ilçede saatler değil
/// saniyeler harcanır.
const OVERPASS_DENEMESI: usize = 2;

/// Geçici sayılan hatalar: sunucu yoğunluğu (429), sunucu hatası (5xx), ağ
/// kesintisi ve zaman aşımı. Bunlarda istek tekrar denenir; kalıcı hatalarda
/// (404, 400...) hemen dönülür.
fn gecici_hata(mesaj: &str) -> bool {
    mesaj.contains("HTTP 429")
        || mesaj.contains("HTTP 5")
        || mesaj.contains("bağlantı kurulamadı")
        || mesaj.contains("bağlantı kesildi")
        || mesaj.contains("zaman aşımı")
        || mesaj.contains("timed out")
}

/// Sunucunun `Retry-After` başlığıyla bildirdiği bekleme süresi, 1-15 sn
/// aralığına kırpılarak. Overpass 429'da "ne kadar bekleyin" bilgisini bu
/// başlıkla verir; körlemesine beklemekten daha isabetlidir.
fn retry_after_sn(mesaj: &str) -> Option<u64> {
    let kalan = mesaj.split("Retry-After:").nth(1)?;
    kalan
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|saniye| saniye.clamp(1, 15))
}

/// İsteği varsayılan deneme sayısıyla tekrar dener.
fn tekrar_dene<T>(islem: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    tekrar_dene_n(ISTEK_DENEMESI, islem)
}

/// İsteği artan bekleme süresiyle (0,8 → 1,6 → 3,2 sn) en fazla
/// `deneme_sayisi` kez dener. Sunucu 429 ile `Retry-After` bildirdiyse o süre
/// kadar beklenir.
fn tekrar_dene_n<T>(
    deneme_sayisi: usize,
    mut islem: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let deneme_sayisi = deneme_sayisi.max(1);
    let mut bekleme = Duration::from_millis(800);
    let mut son_hata = String::new();
    for deneme in 1..=deneme_sayisi {
        match islem() {
            Ok(deger) => return Ok(deger),
            Err(hata) => {
                let tekrar = gecici_hata(&hata) && deneme < deneme_sayisi;
                if let Some(saniye) = retry_after_sn(&hata) {
                    bekleme = Duration::from_secs(saniye);
                }
                son_hata = hata;
                if !tekrar {
                    break;
                }
                thread::sleep(bekleme);
                bekleme *= 2;
            }
        }
    }
    Err(son_hata)
}

fn http_hata(hata: ureq::Error) -> String {
    match hata {
        ureq::Error::Status(kod, cevap) => {
            // Başlık gövde okunmadan alınmalı.
            let bekle = cevap
                .header("Retry-After")
                .and_then(|deger| deger.parse::<u64>().ok());
            let govde = cevap.into_string().unwrap_or_default();
            let ozet: String = govde.chars().take(160).collect();
            match bekle {
                Some(saniye) => {
                    format!("sunucu HTTP {kod} döndü (Retry-After: {saniye} sn): {ozet}")
                }
                None => format!("sunucu HTTP {kod} döndü: {ozet}"),
            }
        }
        ureq::Error::Transport(tasima) => {
            let metin = tasima.to_string();
            let kucuk = metin.to_lowercase();
            if kucuk.contains("timed out") || kucuk.contains("timeout") {
                format!("zaman aşımı: {metin}")
            } else {
                format!("bağlantı kurulamadı: {metin}")
            }
        }
    }
}

fn url_encode(metin: &str) -> String {
    let mut cikti = String::with_capacity(metin.len() * 3);
    for &bayt in metin.as_bytes() {
        match bayt {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                cikti.push(bayt as char);
            }
            _ => cikti.push_str(&format!("%{bayt:02X}")),
        }
    }
    cikti
}

// ---------------------------------------------------------------------------
// Google Places yedeği (hibrit arama)
//
// OSM/Overpass ücretsiz ve verisi saklanabilir; ancak Türkiye'de kapsama
// boşlukları var. Yedek katman yalnızca OSM'in boş döndüğü ilçelerde devreye
// girer ve kendi istek bütçesini tutar: Google'ın ücretsiz tavanı (Text Search
// Enterprise 1.000/ay) aşılmasın diye günlük/aylık sayaç `google_kota.json`
// dosyasında saklanır. Google sonuçları önbelleğe alınmaz (şartlar yalnızca
// place ID'nin kalıcı saklanmasına izin verir); bizde her tarama canlı çeker.
// ---------------------------------------------------------------------------

const GOOGLE_TEXT_SEARCH: &str = "https://places.googleapis.com/v1/places:searchText";
const GOOGLE_KOTA_DOSYASI: &str = "google_kota.json";
/// Tek taramada en fazla kaç ilçe için Google'a düşülür; il geneli tarama
/// 39 ilçeyi gezdiği için bütçe tek taramada tükenmesin.
const GOOGLE_TARAMA_SINIRI: usize = 5;

/// Google anahtarı ve istek bütçesi. Değerler ortam değişkeninden, yoksa
/// çalışma dizinindeki `.env` dosyasından okunur.
struct GoogleAyar {
    anahtar: String,
    gunluk_limit: usize,
    aylik_limit: usize,
    /// Tarama başına en fazla kaç sayfa okunacak (1-3 · sayfa başına 20 kayıt).
    sayfa_siniri: usize,
}

impl GoogleAyar {
    fn oku() -> Self {
        let anahtar = ayar_degeri("GOOGLE_PLACES_API_KEY")
            .or_else(|| ayar_degeri("GOOGLE_MAPS_API_KEY"))
            .unwrap_or_default();
        let gunluk_limit = ayar_degeri("GOOGLE_GUNLUK_LIMIT")
            .and_then(|deger| deger.parse().ok())
            .unwrap_or(10);
        let aylik_limit = ayar_degeri("GOOGLE_AYLIK_LIMIT")
            .and_then(|deger| deger.parse().ok())
            .unwrap_or(200);
        // Her sayfa ayrı faturalanır; varsayılan 2 sayfa (40 kayıt) yeterli.
        let sayfa_siniri = ayar_degeri("GOOGLE_SAYFA")
            .and_then(|deger| deger.parse::<usize>().ok())
            .unwrap_or(2)
            .clamp(1, 3);
        Self {
            anahtar,
            gunluk_limit,
            aylik_limit,
            sayfa_siniri,
        }
    }

    fn acik(&self) -> bool {
        !self.anahtar.trim().is_empty()
    }
}

/// Önce ortam değişkeni, sonra `.env` dosyası (ANAHTAR=deger satırları).
fn ayar_degeri(ad: &str) -> Option<String> {
    if let Ok(deger) = std::env::var(ad) {
        let deger = deger.trim().to_string();
        if !deger.is_empty() {
            return Some(deger);
        }
    }
    let icerik = fs::read_to_string(".env").ok()?;
    for satir in icerik.lines() {
        let satir = satir.trim();
        if satir.is_empty() || satir.starts_with('#') {
            continue;
        }
        if let Some((anahtar, deger)) = satir.split_once('=') {
            if anahtar.trim() == ad {
                let deger = deger.trim().trim_matches('"').trim_matches('\'');
                if !deger.is_empty() {
                    return Some(deger.to_string());
                }
            }
        }
    }
    None
}

/// Günlük/aylık istek sayacı; `google_kota.json` içinde tutulur.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct GoogleKota {
    gun: String,
    gunluk: usize,
    ay: String,
    aylik: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KotaKarari {
    Izin,
    GunlukDolu,
    AylikDolu,
}

/// Kotadan bir istek ayırır; gün/ay değiştiyse sayaçları sıfırlar. İstek
/// gönderilmeden önce sayılır ki hata durumunda bütçe aşılmış olmasın.
fn kota_karari(
    kota: &mut GoogleKota,
    bugun: &str,
    bu_ay: &str,
    gunluk_limit: usize,
    aylik_limit: usize,
) -> KotaKarari {
    if kota.gun != bugun {
        kota.gun = bugun.to_string();
        kota.gunluk = 0;
    }
    if kota.ay != bu_ay {
        kota.ay = bu_ay.to_string();
        kota.aylik = 0;
    }
    if kota.aylik >= aylik_limit {
        return KotaKarari::AylikDolu;
    }
    if kota.gunluk >= gunluk_limit {
        return KotaKarari::GunlukDolu;
    }
    kota.gunluk += 1;
    kota.aylik += 1;
    KotaKarari::Izin
}

fn kota_oku() -> GoogleKota {
    fs::read_to_string(GOOGLE_KOTA_DOSYASI)
        .ok()
        .and_then(|metin| serde_json::from_str(&metin).ok())
        .unwrap_or_default()
}

fn kota_yaz(kota: &GoogleKota) {
    if let Ok(metin) = serde_json::to_string_pretty(kota) {
        let _ = fs::write(GOOGLE_KOTA_DOSYASI, metin);
    }
}

fn bugun_anahtari() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn ay_anahtari() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}

/// Google'ın "Monday: 9:00 AM – 6:00 PM" gibi gün açıklamalarını OSM
/// `opening_hours` biçimine çevirir; ardışık aynı günler tek aralıkta toplanır
/// ("Mo-Fr 09:00-18:00; Sa 10:00-14:00; Su off").
fn google_saatleri_osm(gunler: &[String]) -> String {
    const KODLAR: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let mut araliklar: Vec<Option<String>> = vec![None; 7];
    for (sira, satir) in gunler.iter().enumerate().take(7) {
        let Some((_, deger)) = satir.split_once(':') else {
            continue;
        };
        let deger = deger.trim();
        let kucuk = deger.to_lowercase();
        if kucuk.contains("closed") {
            continue;
        }
        if kucuk.contains("24 hours") {
            araliklar[sira] = Some("00:00-24:00".to_string());
            continue;
        }
        let parcalar: Vec<&str> = deger.split(['–', '-']).map(str::trim).collect();
        if parcalar.len() != 2 {
            continue;
        }
        let (Some(bas), Some(son)) = (google_saat(parcalar[0]), google_saat(parcalar[1])) else {
            continue;
        };
        // "12:00 AM" bitişi gece yarısıdır; OSM bunu gün sonu 24:00 olarak yazar.
        let son = if son == "00:00" {
            "24:00".to_string()
        } else {
            son
        };
        araliklar[sira] = Some(format!("{bas}-{son}"));
    }

    if araliklar.iter().all(Option::is_none) {
        return String::new();
    }
    if araliklar
        .iter()
        .all(|aralik| aralik.as_deref() == Some("00:00-24:00"))
    {
        return "24/7".to_string();
    }

    let mut parcalar: Vec<String> = Vec::new();
    let mut gun = 0usize;
    while gun < 7 {
        let deger = araliklar[gun].clone();
        let mut son = gun;
        while son + 1 < 7 && araliklar[son + 1] == deger {
            son += 1;
        }
        let kod = if gun == son {
            KODLAR[gun].to_string()
        } else {
            format!("{}-{}", KODLAR[gun], KODLAR[son])
        };
        parcalar.push(match deger {
            Some(aralik) => format!("{kod} {aralik}"),
            None => format!("{kod} off"),
        });
        gun = son + 1;
    }
    parcalar.join("; ")
}

/// "9:00 AM" / "6:00 PM" → "09:00" / "18:00".
fn google_saat(metin: &str) -> Option<String> {
    let metin = metin.trim();
    let (govde, ogleden_sonra) = if let Some(kalan) = metin
        .strip_suffix("AM")
        .or_else(|| metin.strip_suffix("am"))
        .or_else(|| metin.strip_suffix("A.M."))
    {
        (kalan.trim(), false)
    } else {
        let kalan = metin
            .strip_suffix("PM")
            .or_else(|| metin.strip_suffix("pm"))
            .or_else(|| metin.strip_suffix("P.M."))?;
        (kalan.trim(), true)
    };
    let (saat_kismi, dakika_kismi) = govde.split_once(':')?;
    let mut saat: u32 = saat_kismi.trim().parse().ok()?;
    let dakika: u32 = dakika_kismi.trim().parse().ok()?;
    if saat > 12 || dakika > 59 {
        return None;
    }
    if ogleden_sonra && saat < 12 {
        saat += 12;
    }
    if !ogleden_sonra && saat == 12 {
        saat = 0;
    }
    Some(format!("{saat:02}:{dakika:02}"))
}

/// Google Text Search yanıtını işletme listesine çevirir. Web sitesi olan
/// kayıtlar elenir (bu ekranın amacı sitesiz işletmeler), place ID'den
/// kalıcı saklanabilen tek alan olarak yararlanılır.
fn google_isle(ham: &str, sektor: &str) -> Result<(Vec<BusinessLead>, usize), String> {
    let deger: serde_json::Value =
        serde_json::from_str(ham).map_err(|hata| format!("Google yanıtı çözümlenemedi: {hata}"))?;
    let mut isletmeler = Vec::new();
    let mut siteli = 0usize;

    if let Some(yerler) = deger.get("places").and_then(|yerler| yerler.as_array()) {
        for yer in yerler {
            let ad = yer
                .pointer("/displayName/text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if ad.is_empty() {
                continue;
            }
            // Kalıcı olarak kapanmış işletmeye teklif götürmenin anlamı yok.
            if yer.get("businessStatus").and_then(|v| v.as_str()) == Some("CLOSED_PERMANENTLY") {
                continue;
            }
            // Google'da da aynı ayrım: gerçek alan adı olan elenir, sosyal
            // medya / ücretsiz site adresi olan sıcak fırsat olarak tutulur.
            let site_uri = yer
                .get("websiteUri")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let site = if site_uri.is_empty() {
                SiteDurumu::Yok
            } else {
                match site_durumu_bul(site_uri) {
                    Some(durum) => durum,
                    None => {
                        siteli += 1;
                        continue;
                    }
                }
            };
            let konum = match (
                yer.pointer("/location/latitude").and_then(|v| v.as_f64()),
                yer.pointer("/location/longitude").and_then(|v| v.as_f64()),
            ) {
                (Some(enlem), Some(boylam)) => Some((enlem, boylam)),
                _ => None,
            };
            let saatler = yer
                .pointer("/regularOpeningHours/weekdayDescriptions")
                .and_then(|v| v.as_array())
                .map(|gunler| {
                    let gunler: Vec<String> = gunler
                        .iter()
                        .filter_map(|gun| gun.as_str().map(str::to_string))
                        .collect();
                    google_saatleri_osm(&gunler)
                })
                .unwrap_or_default();
            let kimlik = yer.get("id").and_then(|v| v.as_str()).unwrap_or("");
            isletmeler.push(BusinessLead {
                name: ad.to_string(),
                address: yer
                    .get("formattedAddress")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                category: sektor.to_string(),
                phone: yer
                    .get("nationalPhoneNumber")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                selected: true,
                kaynak: if kimlik.is_empty() {
                    "https://www.google.com/maps".to_string()
                } else {
                    format!("https://www.google.com/maps/place/?q=place_id:{kimlik}")
                },
                konum,
                calisma_saatleri: saatler,
                // Google kaydı zaten doğrulanmış işletme: +10 kaynak bonusu var.
                kayit_dolulugu: 0,
                site,
                site_linki: site_uri.to_string(),
                son_guncelleme_gun: None,
                inceleme_sayisi: yer
                    .get("userRatingCount")
                    .and_then(|v| v.as_u64())
                    .map(|sayi| sayi.min(u32::MAX as u64) as u32),
                yildiz_puani: yer
                    .get("rating")
                    .and_then(|v| v.as_f64())
                    .map(|puan| puan as f32),
                durum: musteriler::Durum::Yeni,
                demo_yolu: None,
                mesaj_hazir: false,
                yayin_linki: None,
                karne_linki: None,
                mesaj_varyanti: None,
            });
        }
    }

    leads_sirala(&mut isletmeler);
    // İki küme ayrı kotalarla çekildiği için toplam sınır burada uygulanır:
    // en yüksek potansiyelli SONUC_SINIRI kayıt kalır.
    isletmeler.truncate(SONUC_SINIRI);
    Ok((isletmeler, siteli))
}

/// Google Places Text Search isteği; alan maskesi Enterprise kademesine düşer
/// (websiteUri + çalışma saatleri), bu yüzden yalnızca bütçe varsa çağrılır.
/// Text Search istek gövdesi. `pageSize` 1-20 arasıdır ve kullanımdan
/// kaldırılan `maxResultCount` yerine kullanılır; `pageToken` bir sonraki
/// sayfayı ister (Google en fazla 60 sonuç/3 sayfa döndürür).
fn google_govdesi(
    il: &str,
    ilce: &str,
    sektor: &str,
    sayfa_jetonu: Option<&str>,
) -> serde_json::Value {
    let mut govde = serde_json::json!({
        "textQuery": format!("{sektor} {ilce} {il}"),
        "languageCode": "tr",
        "regionCode": "TR",
        "pageSize": SONUC_SINIRI,
    });
    if let Some(jeton) = sayfa_jetonu {
        govde["pageToken"] = serde_json::Value::String(jeton.to_string());
    }
    govde
}

fn google_arama(
    ajan: &ureq::Agent,
    anahtar: &str,
    il: &str,
    ilce: &str,
    sektor: &str,
    sayfa_jetonu: Option<&str>,
) -> Result<(Vec<BusinessLead>, usize, Option<String>), String> {
    let govde = google_govdesi(il, ilce, sektor, sayfa_jetonu);
    let gonder = || {
        ajan.post(GOOGLE_TEXT_SEARCH)
            .set("Content-Type", "application/json")
            .set("X-Goog-Api-Key", anahtar)
        .set(
            "X-Goog-FieldMask",
            // userRatingCount ve rating Enterprise kademesindedir: mevcut
            // alanlarla aynı ücret kademesi, yani ek maliyet getirmez. Yorum
            // sayısı ve yıldız puanı "gerçek müşteri trafiği var" sinyali
            // olarak puanlamada ve dijital karnede kullanılır.
                "places.id,places.displayName,places.formattedAddress,places.location,places.websiteUri,places.nationalPhoneNumber,places.regularOpeningHours.weekdayDescriptions,places.userRatingCount,places.rating,places.businessStatus",
            )
            .send_string(&govde.to_string())
            .map_err(http_hata)?
            .into_string()
            .map_err(|hata| format!("Google yanıtı okunamadı: {hata}"))
    };
    // Aynı sayfa isteği geçici hatalarda tekrar denenir; kotadan tek pay gider.
    let cevap = tekrar_dene(gonder)?;
    let sonraki_jeton = serde_json::from_str::<serde_json::Value>(&cevap)
        .ok()
        .and_then(|deger| {
            deger
                .get("nextPageToken")
                .and_then(|jeton| jeton.as_str())
                .map(str::to_string)
        });
    let (liste, siteli) = google_isle(&cevap, sektor)?;
    Ok((liste, siteli, sonraki_jeton))
}

/// Google Places sorgusu. `sebep` günlüğe yazılır ("OSM'de kayıt yok" veya
/// "Google kaynağı seçili"). Anahtar yoksa ya da günlük/aylık bütçe dolduysa
/// `None` döner ve günlüğe neden atlandığı yazılır.
#[allow(clippy::too_many_arguments)]
fn google_yedek(
    tx: &Sender<Mesaj>,
    ajan: &ureq::Agent,
    ayar: &GoogleAyar,
    kota: &mut GoogleKota,
    il: &str,
    ilce: &str,
    sektor: &str,
    sebep: &str,
) -> Option<Vec<BusinessLead>> {
    if !ayar.acik() {
        return None;
    }
    let (bugun, bu_ay) = (bugun_anahtari(), ay_anahtari());
    let mut toplam: Vec<BusinessLead> = Vec::new();
    let mut gorulen: HashSet<String> = HashSet::new();
    let mut siteli = 0usize;
    let mut jeton: Option<String> = None;
    let mut sayfa = 0usize;
    let mut kota_engeli = false;

    // Her sayfa ayrı istek ve ayrı faturalanır; bu yüzden sayfa başına kotadan
    // pay ayrılır, 20 işletme dolunca veya jeton bitince erken çıkılır.
    while sayfa < ayar.sayfa_siniri && toplam.len() < SONUC_SINIRI {
        match kota_karari(kota, &bugun, &bu_ay, ayar.gunluk_limit, ayar.aylik_limit) {
            KotaKarari::Izin => {}
            KotaKarari::GunlukDolu => {
                let _ = tx.send(Mesaj::Log(format!(
                    "[!] Google bütçesi doldu: günlük istek sınırı ({}/{}).",
                    kota.gunluk, ayar.gunluk_limit
                )));
                kota_engeli = true;
                break;
            }
            KotaKarari::AylikDolu => {
                let _ = tx.send(Mesaj::Log(format!(
                    "[!] Google bütçesi doldu: aylık istek sınırı ({}/{}).",
                    kota.aylik, ayar.aylik_limit
                )));
                kota_engeli = true;
                break;
            }
        }
        kota_yaz(kota);
        if sayfa == 0 {
            let _ = tx.send(Mesaj::Log(format!(
                "[>] {ilce} / {il}: {sebep}, Google Places sorgusu (bugün {}/{}).",
                kota.gunluk, ayar.gunluk_limit
            )));
        }
        match google_arama(ajan, &ayar.anahtar, il, ilce, sektor, jeton.as_deref()) {
            Ok((liste, sayfa_siteli, sonraki)) => {
                siteli += sayfa_siteli;
                for lead in liste {
                    if gorulen.insert(lead.kaynak.clone()) {
                        toplam.push(lead);
                    }
                }
                jeton = sonraki;
                sayfa += 1;
                if jeton.is_none() {
                    break;
                }
            }
            Err(hata) => {
                if sayfa == 0 {
                    let _ = tx.send(Mesaj::Log(format!("[!] Google sorgusu başarısız: {hata}")));
                    return None;
                }
                let _ = tx.send(Mesaj::Log(format!(
                    "[!] Google {}. sayfada durdu: {hata}",
                    sayfa + 1
                )));
                break;
            }
        }
    }

    if toplam.is_empty() {
        if kota_engeli {
            return None;
        }
        let _ = tx.send(Mesaj::Log(format!(
            "[i] Google da boş döndü ({siteli} sitesi olan kayıt ayıklandı, {sayfa} sayfa okundu)."
        )));
        return Some(Vec::new());
    }
    incelemeye_gore_sirala(&mut toplam);
    toplam.truncate(SONUC_SINIRI);
    let _ = tx.send(Mesaj::Log(format!(
        "[✓] Google: {} kayıt eklendi ({} sitesi olan kayıt ayıklandı, {sayfa} sayfa okundu).",
        toplam.len(),
        siteli
    )));
    Some(toplam)
}

/// Kayıt Google'dan mı OSM'den mi geldi; bağlantı metni buna göre yazılır.
fn kaynak_etiketi(kaynak: &str) -> &'static str {
    if kaynak.contains("google.com") {
        "Google kaydını aç ↗"
    } else {
        "OSM kaydını aç ↗"
    }
}

fn etiket<'a>(tags: &'a serde_json::Value, anahtar: &str) -> Option<&'a str> {
    tags.get(anahtar)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// "Beşiktaş, İstanbul" için merkez koordinat ve ilçeyi kapsayan yarıçap.
fn konum_bul(ajan: &ureq::Agent, il: &str, ilce: &str) -> Result<Konum, String> {
    let sorgu = format!("{ilce}, {il}, Türkiye");
    let url = format!(
        "{NOMINATIM_URL}?q={}&format=json&limit=1&addressdetails=0",
        url_encode(&sorgu)
    );
    let govde = tekrar_dene(|| {
        ajan.get(&url)
            .call()
            .map_err(http_hata)?
            .into_string()
            .map_err(|hata| format!("konum yanıtı okunamadı: {hata}"))
    })?;
    let liste: Vec<serde_json::Value> = serde_json::from_str(&govde)
        .map_err(|hata| format!("konum yanıtı çözümlenemedi: {hata}"))?;
    let ilk = liste
        .first()
        .ok_or_else(|| format!("'{sorgu}' için konum bulunamadı"))?;
    let enlem = ilk
        .get("lat")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| "konum yanıtında enlem yok".to_string())?;
    let boylam = ilk
        .get("lon")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| "konum yanıtında boylam yok".to_string())?;
    let yaricap_m = ilk
        .get("boundingbox")
        .map(|bb| kapsama_yaricapi(bb, enlem))
        .unwrap_or(3_000);
    // Overpass alan kimliği: relation -> 3_600_000_000 + id, way -> 2_400_000_000 + id
    let alan_id = match ilk.get("osm_type").and_then(|v| v.as_str()) {
        Some("relation") => ilk
            .get("osm_id")
            .and_then(|v| v.as_i64())
            .map(|id| 3_600_000_000 + id as u64),
        Some("way") => ilk
            .get("osm_id")
            .and_then(|v| v.as_i64())
            .map(|id| 2_400_000_000 + id as u64),
        _ => None,
    };
    Ok(Konum {
        enlem,
        boylam,
        yaricap_m,
        alan_id,
    })
}

/// Nominatim sınır kutusundan [güney, kuzey, batı, doğu] yarıçap üretir.
fn kapsama_yaricapi(boundingbox: &serde_json::Value, enlem: f64) -> u32 {
    let sayi = |i: usize| {
        boundingbox
            .get(i)
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
    };
    let (guney, kuzey, bati, dogu) = match (sayi(0), sayi(1), sayi(2), sayi(3)) {
        (Some(g), Some(k), Some(b), Some(d)) => (g, k, b, d),
        _ => return 3_000,
    };
    let dikey = (kuzey - guney).abs() / 2.0 * 111_320.0;
    let yatay = (dogu - bati).abs() / 2.0 * 111_320.0 * enlem.to_radians().cos().abs();
    (dikey.hypot(yatay).clamp(1_200.0, 20_000.0)) as u32
}

/// Demo sayfasında gösterilecek bölüm ailesi.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Demo {
    Menu,
    Kafe,
    Unlu,
    Urun,
    Galeri,
    Klinik,
    Otel,
    Egitim,
    Ofis,
    Servis,
    Eglence,
}

/// Tek adlı işletme sektörü: OSM etiket karşılığı ve demo şablonu.
struct Sektor {
    ad: &'static str,
    /// Arayüzde listenin bölümlendiği grup başlığı.
    grup: &'static str,
    /// Overpass sorgusunda kullanılan (anahtar, regex) çiftleri.
    osm: &'static [(&'static str, &'static str)],
    demo: Demo,
}

/// Türkiye'deki işletme sektörleri: NACE Rev.2.1 faaliyet kolları, esnaf
/// meslek kolları ve Google Business Profile kategorileri (berber, tırnak
/// salonu, oto kaporta, oto muayene, diş laboratuvarı, yoga stüdyosu,
/// tercüme bürosu, ortak çalışma ofisi...) sadeleştirilerek. Her satır tek bir
/// sektördür, bileşik ad yoktur; aynı sıra arayüzdeki listeye yansır.
///
/// `osm` eşlemesi yalnızca gerçekten kullanılan etiketleri içerir (taginfo'da
/// karşılığı doğrulandı); OSM'de ayrı etiketi olmayan yemek türleri (döner,
/// pide, çiğ köfte) bilinçli olarak eklenmedi, çünkü aynı restoran kümesini
/// ikinci kez listelerdi.
const SEKTORLER: &[Sektor] = &[
    // --- Lokanta & Kebap ---
    Sektor {
        ad: "Restoran",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Kebapçı",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Pideci",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Köfteci",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Mantıcı",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Ev Yemekleri",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$")],
    },
    Sektor {
        ad: "Balık Restoranı",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$"), ("shop", "^seafood$")],
    },
    Sektor {
        ad: "Pilavcı",
        grup: "Lokanta & Kebap",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    // --- Fast Food & Büfe ---
    Sektor {
        ad: "Fast Food",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Dönerci",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Lahmacuncu",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Çiğ Köfteci",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Hamburgerci",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Pizza Salonu",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^restaurant$"), ("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Tavukçu",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("amenity", "^fast_food$")],
    },
    Sektor {
        ad: "Büfe",
        grup: "Fast Food & Büfe",
        demo: Demo::Menu,
        osm: &[("shop", "^kiosk$")],
    },
    // --- Kafe & Çay ---
    Sektor {
        ad: "Kafe",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^cafe$")],
    },
    Sektor {
        ad: "Kahvaltı Salonu",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^cafe$")],
    },
    Sektor {
        ad: "Çay Bahçesi",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^cafe$")],
    },
    Sektor {
        ad: "Kıraathane",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^cafe$")],
    },
    Sektor {
        ad: "Kahveci",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^cafe$")],
    },
    Sektor {
        ad: "Nargile Kafe",
        grup: "Kafe & Çay",
        demo: Demo::Kafe,
        osm: &[("amenity", "^hookah_lounge$")],
    },
    // --- Fırın & Tatlı ---
    Sektor {
        ad: "Fırın",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^bakery$")],
    },
    Sektor {
        ad: "Simitçi",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^bakery$")],
    },
    Sektor {
        ad: "Börekçi",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^bakery$"), ("shop", "^pastry$")],
    },
    Sektor {
        ad: "Baklavacı",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^confectionery$"), ("shop", "^pastry$")],
    },
    Sektor {
        ad: "Pastane",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^pastry$")],
    },
    Sektor {
        ad: "Tatlıcı",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("shop", "^confectionery$")],
    },
    Sektor {
        ad: "Dondurmacı",
        grup: "Fırın & Tatlı",
        demo: Demo::Unlu,
        osm: &[("amenity", "^ice_cream$"), ("shop", "^ice_cream$")],
    },
    // --- Bar & Meyhane ---
    Sektor {
        ad: "Bar",
        grup: "Bar & Meyhane",
        demo: Demo::Menu,
        osm: &[("amenity", "^bar$")],
    },
    Sektor {
        ad: "Meyhane",
        grup: "Bar & Meyhane",
        demo: Demo::Menu,
        osm: &[("amenity", "^pub$")],
    },
    Sektor {
        ad: "Şarap Evi",
        grup: "Bar & Meyhane",
        demo: Demo::Urun,
        osm: &[("shop", "^wine$")],
    },
    Sektor {
        ad: "Kokteyl Bar",
        grup: "Bar & Meyhane",
        demo: Demo::Menu,
        osm: &[("amenity", "^bar$")],
    },
    Sektor {
        ad: "Birahane",
        grup: "Bar & Meyhane",
        demo: Demo::Menu,
        osm: &[("amenity", "^pub$"), ("amenity", "^biergarten$")],
    },
    // --- Yemek Servisi ---
    Sektor {
        ad: "Yemek Servisi",
        grup: "Yemek Servisi",
        demo: Demo::Menu,
        osm: &[("craft", "^catering$")],
    },
    // --- Gıda Perakende ---
    Sektor {
        ad: "Market",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^supermarket$")],
    },
    Sektor {
        ad: "Bakkal",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^convenience$")],
    },
    Sektor {
        ad: "Kasap",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^butcher$")],
    },
    Sektor {
        ad: "Manav",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^greengrocer$")],
    },
    Sektor {
        ad: "Şarküteri",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^(deli|cheese|dairy)$")],
    },
    Sektor {
        ad: "Balıkçı",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^seafood$")],
    },
    Sektor {
        ad: "Peynirci",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^cheese$")],
    },
    Sektor {
        ad: "Çay Dükkânı",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^tea$")],
    },
    Sektor {
        ad: "Çikolata",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^chocolate$")],
    },
    Sektor {
        ad: "Kuruyemişçi",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^nuts$")],
    },
    Sektor {
        ad: "Baharatçı",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^spices$")],
    },
    Sektor {
        ad: "Süt Ürünleri",
        grup: "Gıda Perakende",
        demo: Demo::Urun,
        osm: &[("shop", "^dairy$")],
    },
    // --- Yardımcı Gıda ---
    Sektor {
        ad: "Aktar",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^herbalist$")],
    },
    Sektor {
        ad: "Toptancı",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^wholesale$")],
    },
    Sektor {
        ad: "Dondurulmuş Gıda",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^frozen_food$")],
    },
    Sektor {
        ad: "Takviye Gıda",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^nutrition_supplements$")],
    },
    Sektor {
        ad: "Su Bayii",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^water$")],
    },
    Sektor {
        ad: "Sağlıklı Gıda",
        grup: "Yardımcı Gıda",
        demo: Demo::Urun,
        osm: &[("shop", "^health_food$")],
    },
    // --- Giyim & Aksesuar ---
    Sektor {
        ad: "Giyim",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^(clothes|boutique)$")],
    },
    Sektor {
        ad: "Ayakkabı",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^shoes$")],
    },
    Sektor {
        ad: "Çanta",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^bags$")],
    },
    Sektor {
        ad: "Deri",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^leather$")],
    },
    Sektor {
        ad: "Bijuteri",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^accessories$"), ("shop", "^jewelry$")],
    },
    Sektor {
        ad: "Gelinlik",
        grup: "Giyim & Aksesuar",
        demo: Demo::Galeri,
        osm: &[("shop", "^clothes$"), ("shop", "^boutique$")],
    },
    Sektor {
        ad: "İç Giyim",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^clothes$")],
    },
    Sektor {
        ad: "Spor Giyim",
        grup: "Giyim & Aksesuar",
        demo: Demo::Urun,
        osm: &[("shop", "^sports$")],
    },
    // --- Kuyum & Saat ---
    Sektor {
        ad: "Kuyumcu",
        grup: "Kuyum & Saat",
        demo: Demo::Urun,
        osm: &[("shop", "^jewelry$")],
    },
    Sektor {
        ad: "Saatçi",
        grup: "Kuyum & Saat",
        demo: Demo::Urun,
        osm: &[("shop", "^watches$")],
    },
    Sektor {
        ad: "Saat Tamiri",
        grup: "Kuyum & Saat",
        demo: Demo::Servis,
        osm: &[("craft", "^watchmaker$")],
    },
    Sektor {
        ad: "Gümüş",
        grup: "Kuyum & Saat",
        demo: Demo::Urun,
        osm: &[("shop", "^jewelry$")],
    },
    // --- Tekstil & Ev Tekstili ---
    Sektor {
        ad: "Tekstil",
        grup: "Tekstil & Ev Tekstili",
        demo: Demo::Urun,
        osm: &[("shop", "^fabric$")],
    },
    Sektor {
        ad: "Tuhafiye",
        grup: "Tekstil & Ev Tekstili",
        demo: Demo::Urun,
        osm: &[("shop", "^haberdashery$")],
    },
    Sektor {
        ad: "Perdeci",
        grup: "Tekstil & Ev Tekstili",
        demo: Demo::Urun,
        osm: &[("shop", "^curtain$")],
    },
    Sektor {
        ad: "Halı",
        grup: "Tekstil & Ev Tekstili",
        demo: Demo::Urun,
        osm: &[("shop", "^carpet$")],
    },
    // --- Dikiş & Bakım ---
    Sektor {
        ad: "Terzi",
        grup: "Dikiş & Bakım",
        demo: Demo::Servis,
        osm: &[("craft", "^tailor$"), ("shop", "^tailor$")],
    },
    Sektor {
        ad: "Kuru Temizleme",
        grup: "Dikiş & Bakım",
        demo: Demo::Servis,
        osm: &[("shop", "^(dry_cleaning|laundry)$")],
    },
    Sektor {
        ad: "Ayakkabı Tamiri",
        grup: "Dikiş & Bakım",
        demo: Demo::Servis,
        osm: &[("craft", "^shoemaker$")],
    },
    // --- Temizlik & Bakım ---
    Sektor {
        ad: "Halı Yıkama",
        grup: "Temizlik & Bakım",
        demo: Demo::Servis,
        osm: &[("shop", "^laundry$")],
    },
    Sektor {
        ad: "Temizlik Şirketi",
        grup: "Temizlik & Bakım",
        demo: Demo::Servis,
        osm: &[("shop", "^cleaning$"), ("office", "^cleaning$")],
    },
    // --- Mobilya & Dekorasyon ---
    Sektor {
        ad: "Mobilya",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^furniture$")],
    },
    Sektor {
        ad: "Mutfak Mağazası",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^kitchen$")],
    },
    Sektor {
        ad: "Banyo",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^bathroom_furnishing$")],
    },
    Sektor {
        ad: "Yatak",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^bed$")],
    },
    Sektor {
        ad: "Aydınlatma",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^lighting$")],
    },
    Sektor {
        ad: "Züccaciye",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^houseware$")],
    },
    Sektor {
        ad: "İç Dekorasyon",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^interior_decoration$")],
    },
    Sektor {
        ad: "Çerçeve",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^frame$")],
    },
    Sektor {
        ad: "Süpürge Mağazası",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^vacuum_cleaner$")],
    },
    Sektor {
        ad: "Perde & Stor",
        grup: "Mobilya & Dekorasyon",
        demo: Demo::Urun,
        osm: &[("shop", "^window_blind$")],
    },
    // --- Yapı & Hırdavat ---
    Sektor {
        ad: "Boya Bayii",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^paint$")],
    },
    Sektor {
        ad: "Nalbur",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^(hardware|doityourself)$")],
    },
    Sektor {
        ad: "Seramik",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^tiles$")],
    },
    Sektor {
        ad: "Parke",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^flooring$")],
    },
    Sektor {
        ad: "Şömine",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^fireplace$")],
    },
    Sektor {
        ad: "Elektrik Malzemeleri",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^electrical$")],
    },
    Sektor {
        ad: "İnşaat Malzemeleri",
        grup: "Yapı & Hırdavat",
        demo: Demo::Urun,
        osm: &[("shop", "^doityourself$")],
    },
    // --- Bahçe & Çiçek ---
    Sektor {
        ad: "Bahçe Merkezi",
        grup: "Bahçe & Çiçek",
        demo: Demo::Urun,
        osm: &[("shop", "^garden_centre$")],
    },
    Sektor {
        ad: "Çiçekçi",
        grup: "Bahçe & Çiçek",
        demo: Demo::Urun,
        osm: &[("shop", "^florist$")],
    },
    // --- Elektronik & Teknoloji ---
    Sektor {
        ad: "Elektronik",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^electronics$")],
    },
    Sektor {
        ad: "Telefon",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^mobile_phone$")],
    },
    Sektor {
        ad: "Bilgisayar",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^computer$")],
    },
    Sektor {
        ad: "Beyaz Eşya",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^appliance$")],
    },
    Sektor {
        ad: "Kamera Mağazası",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^camera$")],
    },
    Sektor {
        ad: "Ses Sistemleri",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^hifi$")],
    },
    Sektor {
        ad: "Oyun Konsolu",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^video_games$")],
    },
    Sektor {
        ad: "Elektronik Tamiri",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Servis,
        osm: &[("craft", "^electronics_repair$")],
    },
    Sektor {
        ad: "Güvenlik Kamerası",
        grup: "Elektronik & Teknoloji",
        demo: Demo::Urun,
        osm: &[("shop", "^security$")],
    },
    // --- Kitap & Kırtasiye ---
    Sektor {
        ad: "Kitapçı",
        grup: "Kitap & Kırtasiye",
        demo: Demo::Urun,
        osm: &[("shop", "^books$")],
    },
    Sektor {
        ad: "Kırtasiye",
        grup: "Kitap & Kırtasiye",
        demo: Demo::Urun,
        osm: &[("shop", "^stationery$")],
    },
    Sektor {
        ad: "Fotokopi",
        grup: "Kitap & Kırtasiye",
        demo: Demo::Urun,
        osm: &[("shop", "^copyshop$")],
    },
    Sektor {
        ad: "Oyuncakçı",
        grup: "Kitap & Kırtasiye",
        demo: Demo::Urun,
        osm: &[("shop", "^toys$")],
    },
    Sektor {
        ad: "Sanat Malzemeleri",
        grup: "Kitap & Kırtasiye",
        demo: Demo::Urun,
        osm: &[("shop", "^art$")],
    },
    // --- Hobi & Koleksiyon ---
    Sektor {
        ad: "Antikacı",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^antiques$")],
    },
    Sektor {
        ad: "İkinci El",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^second_hand$")],
    },
    Sektor {
        ad: "Kamp Malzemeleri",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^outdoor$")],
    },
    Sektor {
        ad: "Plak & CD",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^music$")],
    },
    Sektor {
        ad: "Parti Malzemeleri",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^party$")],
    },
    Sektor {
        ad: "Avcılık",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^hunting$")],
    },
    Sektor {
        ad: "Balık Malzemeleri",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^fishing$")],
    },
    Sektor {
        ad: "Hobi Mağazası",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^hobby$")],
    },
    Sektor {
        ad: "Dalış Malzemeleri",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^scuba_diving$")],
    },
    Sektor {
        ad: "Model & Maket",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^model$")],
    },
    Sektor {
        ad: "Koleksiyon",
        grup: "Hobi & Koleksiyon",
        demo: Demo::Urun,
        osm: &[("shop", "^collector$")],
    },
    // --- Spor & Müzik ---
    Sektor {
        ad: "Spor Mağazası",
        grup: "Spor & Müzik",
        demo: Demo::Urun,
        osm: &[("shop", "^sports$")],
    },
    Sektor {
        ad: "Bisikletçi",
        grup: "Spor & Müzik",
        demo: Demo::Urun,
        osm: &[("shop", "^bicycle$")],
    },
    Sektor {
        ad: "Müzik Aletleri",
        grup: "Spor & Müzik",
        demo: Demo::Urun,
        osm: &[("shop", "^musical_instrument$")],
    },
    // --- Otomotiv ---
    Sektor {
        ad: "Oto Galeri",
        grup: "Otomotiv",
        demo: Demo::Urun,
        osm: &[("shop", "^car$")],
    },
    Sektor {
        ad: "Oto Yedek Parça",
        grup: "Otomotiv",
        demo: Demo::Urun,
        osm: &[("shop", "^car_parts$")],
    },
    Sektor {
        ad: "Motosiklet",
        grup: "Otomotiv",
        demo: Demo::Urun,
        osm: &[("shop", "^motorcycle$")],
    },
    Sektor {
        ad: "Karavan",
        grup: "Otomotiv",
        demo: Demo::Urun,
        osm: &[("shop", "^caravan$")],
    },
    Sektor {
        ad: "Kamyon & Ticari Araç",
        grup: "Otomotiv",
        demo: Demo::Urun,
        osm: &[("shop", "^truck$")],
    },
    // --- Medikal & Optik ---
    Sektor {
        ad: "Optik",
        grup: "Medikal & Optik",
        demo: Demo::Urun,
        osm: &[("shop", "^optician$")],
    },
    Sektor {
        ad: "Medikal",
        grup: "Medikal & Optik",
        demo: Demo::Urun,
        osm: &[("shop", "^medical_supply$")],
    },
    Sektor {
        ad: "İşitme Cihazı",
        grup: "Medikal & Optik",
        demo: Demo::Urun,
        osm: &[("shop", "^hearing_aids$")],
    },
    Sektor {
        ad: "Odyoloji",
        grup: "Medikal & Optik",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^audiologist$")],
    },
    // --- Evcil Hayvan ---
    Sektor {
        ad: "Pet Shop",
        grup: "Evcil Hayvan",
        demo: Demo::Urun,
        osm: &[("shop", "^pet$")],
    },
    Sektor {
        ad: "Pet Kuaför",
        grup: "Evcil Hayvan",
        demo: Demo::Servis,
        osm: &[("shop", "^pet_grooming$")],
    },
    Sektor {
        ad: "Pet Otel",
        grup: "Evcil Hayvan",
        demo: Demo::Servis,
        osm: &[("amenity", "^animal_boarding$")],
    },
    Sektor {
        ad: "Pet Eğitimi",
        grup: "Evcil Hayvan",
        demo: Demo::Servis,
        osm: &[("amenity", "^animal_training$")],
    },
    Sektor {
        ad: "Akvaryumcu",
        grup: "Evcil Hayvan",
        demo: Demo::Urun,
        osm: &[("shop", "^aquarium$"), ("shop", "^pet$")],
    },
    // --- Yaşam & Diğer ---
    Sektor {
        ad: "Tütüncü",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^tobacco$")],
    },
    Sektor {
        ad: "İçkici",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^alcohol$")],
    },
    Sektor {
        ad: "Depolama",
        grup: "Yaşam & Diğer",
        demo: Demo::Servis,
        osm: &[("shop", "^storage_rental$")],
    },
    Sektor {
        ad: "Bebek Mağazası",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^baby_goods$")],
    },
    Sektor {
        ad: "Gazete Bayii",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^newsagent$")],
    },
    Sektor {
        ad: "Şans Oyunları",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^lottery$")],
    },
    Sektor {
        ad: "Video Kiralama",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^video$")],
    },
    Sektor {
        ad: "Hediyelik Eşya",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^gift$")],
    },
    Sektor {
        ad: "Elektronik Sigara",
        grup: "Yaşam & Diğer",
        demo: Demo::Urun,
        osm: &[("shop", "^e-cigarette$")],
    },
    Sektor {
        ad: "Cenaze Hizmeti",
        grup: "Yaşam & Diğer",
        demo: Demo::Servis,
        osm: &[("shop", "^funeral_directors$")],
    },
    // --- Sağlık Kuruluşları ---
    Sektor {
        ad: "Eczane",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Urun,
        osm: &[("amenity", "^pharmacy$")],
    },
    Sektor {
        ad: "Diş Kliniği",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("amenity", "^dentist$"), ("healthcare", "^dentist$")],
    },
    Sektor {
        ad: "Klinik",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[
            ("amenity", "^(clinic|doctors)$"),
            ("healthcare", "^clinic$"),
        ],
    },
    Sektor {
        ad: "Hastane",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("amenity", "^hospital$")],
    },
    Sektor {
        ad: "Veteriner",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("amenity", "^veterinary$")],
    },
    Sektor {
        ad: "Laboratuvar",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^laboratory$")],
    },
    Sektor {
        ad: "Diyaliz Merkezi",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^dialysis$")],
    },
    Sektor {
        ad: "Göz Kliniği",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^optometrist$")],
    },
    Sektor {
        ad: "Kan Merkezi",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^blood_donation$")],
    },
    Sektor {
        ad: "Diş Laboratuvarı",
        grup: "Sağlık Kuruluşları",
        demo: Demo::Klinik,
        osm: &[("craft", "^dental_technician$")],
    },
    // --- Terapi & Rehabilitasyon ---
    Sektor {
        ad: "Fizyoterapi",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^physiotherapist$")],
    },
    Sektor {
        ad: "Psikolog",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^psychotherapist$")],
    },
    Sektor {
        ad: "Rehabilitasyon",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^rehabilitation$")],
    },
    Sektor {
        ad: "Podolog",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^podiatrist$")],
    },
    Sektor {
        ad: "Konuşma Terapisi",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^speech_therapist$")],
    },
    Sektor {
        ad: "Alternatif Tıp",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^alternative$")],
    },
    Sektor {
        ad: "Ergoterapi",
        grup: "Terapi & Rehabilitasyon",
        demo: Demo::Klinik,
        osm: &[("healthcare", "^occupational_therapist$")],
    },
    // --- Güzellik & Bakım ---
    Sektor {
        ad: "Kuaför",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^hairdresser$")],
    },
    Sektor {
        ad: "Güzellik Salonu",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^beauty$"), ("leisure", "^spa$")],
    },
    Sektor {
        ad: "Masaj",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^massage$")],
    },
    Sektor {
        ad: "Dövme",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^tattoo$")],
    },
    Sektor {
        ad: "Hamam",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("amenity", "^public_bath$")],
    },
    Sektor {
        ad: "Fotoğrafçı",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("craft", "^photographer$"), ("shop", "^photo$")],
    },
    Sektor {
        ad: "Kozmetik",
        grup: "Güzellik & Bakım",
        demo: Demo::Urun,
        osm: &[("shop", "^cosmetics$")],
    },
    Sektor {
        ad: "Parfümeri",
        grup: "Güzellik & Bakım",
        demo: Demo::Urun,
        osm: &[("shop", "^perfumery$")],
    },
    Sektor {
        ad: "Berber",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^hairdresser$")],
    },
    Sektor {
        ad: "Tırnak Stüdyosu",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^nails$"), ("shop", "^beauty$")],
    },
    Sektor {
        ad: "Solaryum",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("leisure", "^tanning_salon$")],
    },
    Sektor {
        ad: "Makyaj Stüdyosu",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^beauty$")],
    },
    Sektor {
        ad: "Epilasyon",
        grup: "Güzellik & Bakım",
        demo: Demo::Galeri,
        osm: &[("shop", "^beauty$")],
    },
    // --- Konaklama ---
    Sektor {
        ad: "Otel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^hotel$")],
    },
    Sektor {
        ad: "Pansiyon",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^guest_house$")],
    },
    Sektor {
        ad: "Hostel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^hostel$")],
    },
    Sektor {
        ad: "Kamp Alanı",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^(camp_site|caravan_site)$")],
    },
    Sektor {
        ad: "Apart Otel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^apartment$")],
    },
    Sektor {
        ad: "Bungalov",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^chalet$")],
    },
    Sektor {
        ad: "Plaj Tesisi",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("leisure", "^beach_resort$")],
    },
    Sektor {
        ad: "Motel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^motel$")],
    },
    Sektor {
        ad: "Butik Otel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^hotel$")],
    },
    Sektor {
        ad: "Termal Otel",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^hotel$")],
    },
    Sektor {
        ad: "Karavan Parkı",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^caravan_site$")],
    },
    Sektor {
        ad: "Dağ Evi",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^alpine_hut$"), ("tourism", "^chalet$")],
    },
    Sektor {
        ad: "Tatil Köyü",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("tourism", "^resort$")],
    },
    Sektor {
        ad: "Öğrenci Yurdu",
        grup: "Konaklama",
        demo: Demo::Otel,
        osm: &[("amenity", "^dormitory$")],
    },
    // --- Eğitim ---
    Sektor {
        ad: "Kurs",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("office", "^educational_institution$")],
    },
    Sektor {
        ad: "Sürücü Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^driving_school$")],
    },
    Sektor {
        ad: "Dil Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^language_school$")],
    },
    Sektor {
        ad: "Müzik Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^music_school$")],
    },
    Sektor {
        ad: "Kreş",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^(kindergarten|childcare)$")],
    },
    Sektor {
        ad: "Etüt Merkezi",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^prep_school$")],
    },
    Sektor {
        ad: "Okul",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^school$")],
    },
    Sektor {
        ad: "Kolej",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^college$")],
    },
    Sektor {
        ad: "Üniversite",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^university$")],
    },
    Sektor {
        ad: "Dans Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("leisure", "^dance$")],
    },
    Sektor {
        ad: "Yüzme Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("leisure", "^swimming_pool$")],
    },
    Sektor {
        ad: "Sanat Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^arts_centre$")],
    },
    Sektor {
        ad: "Anaokulu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^kindergarten$")],
    },
    Sektor {
        ad: "Meslek Kursu",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^training$")],
    },
    Sektor {
        ad: "Dershane",
        grup: "Eğitim",
        demo: Demo::Egitim,
        osm: &[("amenity", "^prep_school$")],
    },
    // --- Yapı & Emlak ---
    Sektor {
        ad: "Mimarlık",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[("office", "^architect$")],
    },
    Sektor {
        ad: "Mühendislik",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[("office", "^engineer$")],
    },
    Sektor {
        ad: "İnşaat",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[
            ("office", "^(construction_company|company)$"),
            ("craft", "^(builder|scaffolder)$"),
        ],
    },
    Sektor {
        ad: "Emlak",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[("office", "^estate_agent$")],
    },
    Sektor {
        ad: "Harita Mühendisi",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[("office", "^surveyor$")],
    },
    Sektor {
        ad: "Bina Yönetimi",
        grup: "Yapı & Emlak",
        demo: Demo::Ofis,
        osm: &[("office", "^property_management$")],
    },
    // --- Hukuk & Mali ---
    Sektor {
        ad: "Muhasebe",
        grup: "Hukuk & Mali",
        demo: Demo::Ofis,
        osm: &[("office", "^accountant$")],
    },
    Sektor {
        ad: "Avukat",
        grup: "Hukuk & Mali",
        demo: Demo::Ofis,
        osm: &[("office", "^lawyer$")],
    },
    Sektor {
        ad: "Noter",
        grup: "Hukuk & Mali",
        demo: Demo::Ofis,
        osm: &[("office", "^notary$")],
    },
    Sektor {
        ad: "Vergi Danışmanı",
        grup: "Hukuk & Mali",
        demo: Demo::Ofis,
        osm: &[("office", "^tax_advisor$")],
    },
    Sektor {
        ad: "Tercüme Bürosu",
        grup: "Hukuk & Mali",
        demo: Demo::Ofis,
        osm: &[("office", "^translator$")],
    },
    // --- Finans & Resmi İşlemler ---
    Sektor {
        ad: "Banka",
        grup: "Finans & Resmi İşlemler",
        demo: Demo::Ofis,
        osm: &[("amenity", "^bank$")],
    },
    Sektor {
        ad: "Döviz Bürosu",
        grup: "Finans & Resmi İşlemler",
        demo: Demo::Ofis,
        osm: &[("amenity", "^bureau_de_change$")],
    },
    Sektor {
        ad: "Sigorta",
        grup: "Finans & Resmi İşlemler",
        demo: Demo::Ofis,
        osm: &[("office", "^insurance$")],
    },
    Sektor {
        ad: "Postane",
        grup: "Finans & Resmi İşlemler",
        demo: Demo::Ofis,
        osm: &[("amenity", "^post_office$")],
    },
    Sektor {
        ad: "Finans Danışmanı",
        grup: "Finans & Resmi İşlemler",
        demo: Demo::Ofis,
        osm: &[("office", "^financial$")],
    },
    // --- Kurum & Topluluk ---
    Sektor {
        ad: "Dernek",
        grup: "Kurum & Topluluk",
        demo: Demo::Ofis,
        osm: &[("office", "^association$")],
    },
    Sektor {
        ad: "Vakıf",
        grup: "Kurum & Topluluk",
        demo: Demo::Ofis,
        osm: &[("office", "^ngo$")],
    },
    // --- Bilişim & Kurumsal Destek ---
    Sektor {
        ad: "Yazılım",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^it$")],
    },
    Sektor {
        ad: "Reklam",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^advertising_agency$")],
    },
    Sektor {
        ad: "Matbaa",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Servis,
        osm: &[("craft", "^printer$"), ("shop", "^copyshop$")],
    },
    Sektor {
        ad: "İnsan Kaynakları",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^employment_agency$")],
    },
    Sektor {
        ad: "Danışmanlık",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^consulting$")],
    },
    Sektor {
        ad: "Telekomünikasyon",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^telecommunication$")],
    },
    Sektor {
        ad: "Yayınevi",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^publisher$")],
    },
    Sektor {
        ad: "Gazete",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^newspaper$")],
    },
    Sektor {
        ad: "Güvenlik",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^security$")],
    },
    Sektor {
        ad: "Araştırma",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^research$")],
    },
    Sektor {
        ad: "Grafik Tasarım",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^graphic_design$")],
    },
    Sektor {
        ad: "Ortak Çalışma Ofisi",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^coworking$")],
    },
    Sektor {
        ad: "Web Tasarım",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^it$")],
    },
    Sektor {
        ad: "Sosyal Medya",
        grup: "Bilişim & Kurumsal Destek",
        demo: Demo::Ofis,
        osm: &[("office", "^advertising_agency$")],
    },
    // --- Lojistik & Seyahat ---
    Sektor {
        ad: "Nakliyat",
        grup: "Lojistik & Seyahat",
        demo: Demo::Ofis,
        osm: &[("office", "^(logistics|moving_company)$")],
    },
    Sektor {
        ad: "Turizm Acentası",
        grup: "Lojistik & Seyahat",
        demo: Demo::Ofis,
        osm: &[("shop", "^travel_agency$"), ("office", "^travel_agent$")],
    },
    Sektor {
        ad: "Kurye",
        grup: "Lojistik & Seyahat",
        demo: Demo::Servis,
        osm: &[("shop", "^courier$"), ("office", "^logistics$")],
    },
    // --- Oto Servis ---
    Sektor {
        ad: "Oto Tamir",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^car_repair$")],
    },
    Sektor {
        ad: "Lastikçi",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^tyres$")],
    },
    Sektor {
        ad: "Oto Yıkama",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^car_wash$")],
    },
    Sektor {
        ad: "Oto Kiralama",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^car_rental$")],
    },
    Sektor {
        ad: "Akaryakıt",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^fuel$")],
    },
    Sektor {
        ad: "Motosiklet Tamiri",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^motorcycle_repair$")],
    },
    Sektor {
        ad: "Şarj İstasyonu",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^charging_station$")],
    },
    Sektor {
        ad: "Otopark",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^parking$")],
    },
    Sektor {
        ad: "Oto Kaporta",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^car_repair$")],
    },
    Sektor {
        ad: "Oto Döşeme",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("craft", "^upholsterer$")],
    },
    Sektor {
        ad: "Oto Muayene",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("amenity", "^vehicle_inspection$")],
    },
    Sektor {
        ad: "Oto Elektrik",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^car_parts$"), ("shop", "^car_repair$")],
    },
    Sektor {
        ad: "Çekici",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^car_repair$")],
    },
    Sektor {
        ad: "Egzoz",
        grup: "Oto Servis",
        demo: Demo::Servis,
        osm: &[("shop", "^car_repair$")],
    },
    // --- Tesisat & Güvenlik ---
    Sektor {
        ad: "Tesisatçı",
        grup: "Tesisat & Güvenlik",
        demo: Demo::Servis,
        osm: &[("craft", "^plumber$")],
    },
    Sektor {
        ad: "Elektrikçi",
        grup: "Tesisat & Güvenlik",
        demo: Demo::Servis,
        osm: &[("craft", "^electrician$")],
    },
    Sektor {
        ad: "Klima",
        grup: "Tesisat & Güvenlik",
        demo: Demo::Servis,
        osm: &[("craft", "^hvac$")],
    },
    Sektor {
        ad: "Çilingir",
        grup: "Tesisat & Güvenlik",
        demo: Demo::Servis,
        osm: &[("craft", "^(key_cutting|locksmith)$")],
    },
    // --- İnşaat Ustaları ---
    Sektor {
        ad: "Boyacı",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^painter$")],
    },
    Sektor {
        ad: "Marangoz",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^(carpenter|joiner)$")],
    },
    Sektor {
        ad: "Camcı",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^glaziery$")],
    },
    Sektor {
        ad: "Fayansçı",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^tiler$")],
    },
    Sektor {
        ad: "Sıvacı",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^plasterer$")],
    },
    Sektor {
        ad: "Çatıcı",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^roofer$")],
    },
    Sektor {
        ad: "Zemin Döşeme",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^floorer$")],
    },
    Sektor {
        ad: "Taş Ustası",
        grup: "İnşaat Ustaları",
        demo: Demo::Servis,
        osm: &[("craft", "^stonemason$")],
    },
    // --- Metal & Doğrama ---
    Sektor {
        ad: "Demirci",
        grup: "Metal & Doğrama",
        demo: Demo::Servis,
        osm: &[("craft", "^blacksmith$")],
    },
    Sektor {
        ad: "Kaynakçı",
        grup: "Metal & Doğrama",
        demo: Demo::Servis,
        osm: &[("craft", "^welder$")],
    },
    Sektor {
        ad: "PVC Doğrama",
        grup: "Metal & Doğrama",
        demo: Demo::Servis,
        osm: &[("craft", "^window_construction$")],
    },
    Sektor {
        ad: "Metal İşleri",
        grup: "Metal & Doğrama",
        demo: Demo::Servis,
        osm: &[("craft", "^metal_construction$")],
    },
    // --- Çevre & Bakım ---
    Sektor {
        ad: "Peyzaj",
        grup: "Çevre & Bakım",
        demo: Demo::Servis,
        osm: &[("craft", "^gardener$")],
    },
    Sektor {
        ad: "İlaçlama",
        grup: "Çevre & Bakım",
        demo: Demo::Servis,
        osm: &[("craft", "^pest_control$")],
    },
    // --- Spor & Fitness ---
    Sektor {
        ad: "Spor Salonu",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^(fitness_centre|sports_centre)$")],
    },
    Sektor {
        ad: "Sauna",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^sauna$")],
    },
    Sektor {
        ad: "Binicilik",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^horse_riding$")],
    },
    Sektor {
        ad: "Golf",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^golf_course$")],
    },
    Sektor {
        ad: "Buz Pisti",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^ice_rink$")],
    },
    Sektor {
        ad: "Su Parkı",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^water_park$")],
    },
    Sektor {
        ad: "Yoga Stüdyosu",
        grup: "Spor & Fitness",
        demo: Demo::Egitim,
        osm: &[("sport", "^yoga$")],
    },
    Sektor {
        ad: "Pilates Stüdyosu",
        grup: "Spor & Fitness",
        demo: Demo::Egitim,
        osm: &[("sport", "^pilates$")],
    },
    Sektor {
        ad: "Dövüş Sporları",
        grup: "Spor & Fitness",
        demo: Demo::Egitim,
        osm: &[("sport", "^martial_arts$")],
    },
    Sektor {
        ad: "Trambolin Parkı",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^trampoline_park$")],
    },
    Sektor {
        ad: "Halı Saha",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^pitch$")],
    },
    Sektor {
        ad: "Yüzme Havuzu",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("leisure", "^swimming_pool$")],
    },
    Sektor {
        ad: "Atış Poligonu",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("sport", "^shooting$")],
    },
    Sektor {
        ad: "Tenis Kortu",
        grup: "Spor & Fitness",
        demo: Demo::Eglence,
        osm: &[("sport", "^tennis$")],
    },
    // --- Mekân & Etkinlik ---
    Sektor {
        ad: "Düğün Salonu",
        grup: "Mekân & Etkinlik",
        demo: Demo::Eglence,
        osm: &[("amenity", "^(events_venue|community_centre)$")],
    },
    Sektor {
        ad: "Gece Kulübü",
        grup: "Mekân & Etkinlik",
        demo: Demo::Eglence,
        osm: &[("amenity", "^nightclub$")],
    },
    Sektor {
        ad: "Konser Salonu",
        grup: "Mekân & Etkinlik",
        demo: Demo::Eglence,
        osm: &[("amenity", "^music_venue$")],
    },
    Sektor {
        ad: "Konferans Merkezi",
        grup: "Mekân & Etkinlik",
        demo: Demo::Eglence,
        osm: &[("amenity", "^conference_centre$")],
    },
    Sektor {
        ad: "Organizasyon",
        grup: "Mekân & Etkinlik",
        demo: Demo::Ofis,
        osm: &[("office", "^event_management$")],
    },
    Sektor {
        ad: "Fuar Merkezi",
        grup: "Mekân & Etkinlik",
        demo: Demo::Eglence,
        osm: &[("amenity", "^exhibition_centre$")],
    },
    // --- Sinema & Sahne ---
    Sektor {
        ad: "Sinema",
        grup: "Sinema & Sahne",
        demo: Demo::Eglence,
        osm: &[("amenity", "^cinema$")],
    },
    Sektor {
        ad: "Tiyatro",
        grup: "Sinema & Sahne",
        demo: Demo::Eglence,
        osm: &[("amenity", "^theatre$")],
    },
    // --- Oyun & Eğlence ---
    Sektor {
        ad: "Oyun Salonu",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^amusement_arcade$")],
    },
    Sektor {
        ad: "Bowling",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^bowling_alley$")],
    },
    Sektor {
        ad: "Lunapark",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("tourism", "^theme_park$")],
    },
    Sektor {
        ad: "Oyun Parkı",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^playground$")],
    },
    Sektor {
        ad: "Escape Oyunu",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^escape_game$")],
    },
    Sektor {
        ad: "İnternet Kafe",
        grup: "Oyun & Eğlence",
        demo: Demo::Kafe,
        osm: &[("amenity", "^internet_cafe$")],
    },
    Sektor {
        ad: "Karaoke",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("amenity", "^karaoke_box$")],
    },
    Sektor {
        ad: "Go-Kart",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^track$")],
    },
    Sektor {
        ad: "Mini Golf",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^miniature_golf$")],
    },
    Sektor {
        ad: "Kapalı Oyun Alanı",
        grup: "Oyun & Eğlence",
        demo: Demo::Eglence,
        osm: &[("leisure", "^indoor_play$")],
    },
    // --- Kültür & Gezi ---
    Sektor {
        ad: "Müze",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("tourism", "^museum$")],
    },
    Sektor {
        ad: "Sanat Galerisi",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("tourism", "^gallery$")],
    },
    Sektor {
        ad: "Hayvanat Bahçesi",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("tourism", "^zoo$")],
    },
    Sektor {
        ad: "Akvaryum",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("tourism", "^aquarium$")],
    },
    Sektor {
        ad: "Planetaryum",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("amenity", "^planetarium$")],
    },
    Sektor {
        ad: "Marina",
        grup: "Kültür & Gezi",
        demo: Demo::Eglence,
        osm: &[("leisure", "^marina$")],
    },
    // --- Sanayi & Üretim ---
    Sektor {
        ad: "Fabrika",
        grup: "Sanayi & Üretim",
        demo: Demo::Urun,
        osm: &[("man_made", "^works$")],
    },
    Sektor {
        ad: "Şarap Üretimi",
        grup: "Sanayi & Üretim",
        demo: Demo::Urun,
        osm: &[("craft", "^winery$")],
    },
    Sektor {
        ad: "Bira Fabrikası",
        grup: "Sanayi & Üretim",
        demo: Demo::Urun,
        osm: &[("craft", "^brewery$")],
    },
    Sektor {
        ad: "Zeytinyağı",
        grup: "Sanayi & Üretim",
        demo: Demo::Urun,
        osm: &[("craft", "^oil_mill$")],
    },
    Sektor {
        ad: "Ambalaj",
        grup: "Sanayi & Üretim",
        demo: Demo::Urun,
        osm: &[("shop", "^packaging$")],
    },
    // --- Tarım & Hayvancılık ---
    Sektor {
        ad: "Tarım Makineleri",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("shop", "^agrarian$")],
    },
    Sektor {
        ad: "Arıcılık",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("craft", "^beekeeper$")],
    },
    Sektor {
        ad: "Çiftlik",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("landuse", "^farmyard$")],
    },
    Sektor {
        ad: "Sera",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("landuse", "^greenhouse_horticulture$")],
    },
    Sektor {
        ad: "Traktör",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("shop", "^tractor$")],
    },
    Sektor {
        ad: "Zirai İlaç",
        grup: "Tarım & Hayvancılık",
        demo: Demo::Urun,
        osm: &[("shop", "^agrarian$")],
    },
];

/// Uygulamadaki sektör adını OSM anahtar/değer kurallarına çevirir.
fn osm_kurallari(sektor: &str) -> Vec<(&'static str, &'static str)> {
    SEKTORLER
        .iter()
        .find(|tanim| tanim.ad == sektor)
        .map(|tanim| tanim.osm.to_vec())
        .unwrap_or_default()
}

/// Sektörün arayüzdeki grup başlığı.
fn sektor_grubu(sektor: &str) -> &'static str {
    SEKTORLER
        .iter()
        .find(|tanim| tanim.ad == sektor)
        .map(|tanim| tanim.grup)
        .unwrap_or("Diğer")
}

/// Sektörün demo şablon ailesi; tanımsız ada düşülürse perakende şablonu.
fn demo_turu(sektor: &str) -> Demo {
    SEKTORLER
        .iter()
        .find(|tanim| tanim.ad == sektor)
        .map(|tanim| tanim.demo)
        .unwrap_or(Demo::Urun)
}

fn adres_metni(tags: &serde_json::Value, ilce: &str, il: &str) -> String {
    let sokak = etiket(tags, "addr:street");
    let kapi_no = etiket(tags, "addr:housenumber");
    let mahalle = etiket(tags, "addr:suburb")
        .or_else(|| etiket(tags, "addr:neighbourhood"))
        .or_else(|| etiket(tags, "addr:quarter"));

    let cadde = match (sokak, kapi_no) {
        (Some(sokak), Some(no)) => format!("{sokak} No:{no}"),
        (Some(sokak), None) => sokak.to_string(),
        _ => String::new(),
    };
    let yer = match mahalle {
        Some(mahalle) => format!("{mahalle}, {ilce} / {il}"),
        None => format!("{ilce} / {il}"),
    };
    if cadde.is_empty() {
        yer
    } else {
        format!("{cadde}, {yer}")
    }
}

/// URL'nin alan adını döndürür: "https://www.instagram.com/kafe" →
/// "instagram.com". Şema, yol ve sorgu atılır; `www.` kırpılır.
fn url_alan_adi(url: &str) -> String {
    let temiz = url.trim();
    let govde = temiz
        .split_once("://")
        .map(|(_, kalan)| kalan)
        .unwrap_or(temiz);
    let alan = govde.split(['/', '?', '#']).next().unwrap_or("");
    alan.trim_start_matches("www.")
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// Defterde ve CSV'de kullanılan kısa web varlığı kodu.
fn site_kodu(site: SiteDurumu) -> &'static str {
    match site {
        SiteDurumu::Yok => "yok",
        SiteDurumu::Sosyal => "sosyal",
        SiteDurumu::Ucretsiz => "ucretsiz",
    }
}

/// Bir adresin ne tür bir web varlığı olduğunu söyler. `None` = gerçek alan
/// adı (yani işletmenin kendi sitesi) veya boş adres.
fn site_durumu_bul(url: &str) -> Option<SiteDurumu> {
    const SOSYAL: [&str; 8] = [
        "instagram.com",
        "facebook.com",
        "fb.com",
        "linktr.ee",
        "tiktok.com",
        "wa.me",
        "twitter.com",
        "x.com",
    ];
    const UCRETSIZ: [&str; 6] = [
        "business.site",
        "wixsite.com",
        "blogspot.com",
        "wordpress.com",
        "weebly.com",
        "webnode.com",
    ];
    let alan = url_alan_adi(url);
    if alan.is_empty() {
        return None;
    }
    let eslesir = |liste: &[&str]| {
        liste
            .iter()
            .any(|hedef| alan == *hedef || alan.ends_with(&format!(".{hedef}")))
    };
    if eslesir(&SOSYAL) {
        return Some(SiteDurumu::Sosyal);
    }
    if eslesir(&UCRETSIZ) {
        return Some(SiteDurumu::Ucretsiz);
    }
    None
}

/// Kaydın ek bilgi doluluğu (0-10): puanlamada kullanılan alanlar (isim,
/// telefon, saat, adres) ve kaynak/tür etiketleri hariç tutulur; kalan her iki
/// etiket 2 puan eder. `cuisine`, `brand`, `wheelchair`, `payment:*` gibi
/// etiketler işletmenin gerçekten işletildiğinin ve kaydının takip
/// edildiğinin işaretidir.
fn kayit_dolulugu(tags: &serde_json::Value) -> u8 {
    const SAYILMAYAN: [&str; 14] = [
        "name",
        "amenity",
        "shop",
        "office",
        "tourism",
        "leisure",
        "healthcare",
        "craft",
        "club",
        "building",
        "phone",
        "contact:phone",
        "opening_hours",
        "website",
    ];
    let Some(harita) = tags.as_object() else {
        return 0;
    };
    let ek = harita
        .keys()
        .filter(|anahtar| {
            !SAYILMAYAN.contains(&anahtar.as_str())
                && !anahtar.starts_with("addr:")
                && !anahtar.starts_with("contact:")
                && !anahtar.starts_with("source")
                && !anahtar.starts_with("ref")
        })
        .count();
    (ek.min(5) as u8) * 2
}

/// Overpass yanıtını işletme listesine çevirir; (sitesiz işletmeler, sitesi olan sayısı).
fn overpass_isle(
    govde: &str,
    il: &str,
    ilce: &str,
    sektor: &str,
) -> Result<(Vec<BusinessLead>, usize), String> {
    let kok: serde_json::Value = serde_json::from_str(govde)
        .map_err(|hata| format!("tarama yanıtı çözümlenemedi: {hata}"))?;
    let elemanlar = kok
        .get("elements")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "tarama yanıtında 'elements' alanı yok".to_string())?;

    let mut isletmeler: Vec<BusinessLead> = Vec::new();
    let mut gorulen: HashSet<String> = HashSet::new();
    let mut siteli = 0usize;

    for eleman in elemanlar {
        let Some(tags) = eleman.get("tags") else {
            continue;
        };
        let Some(ad) = etiket(tags, "name") else {
            continue;
        };
        // Web varlığını üçe ayır: gerçek sitesi olanı ele, sosyal medya veya
        // ücretsiz site kullananı en yüksek niyet sinyali olarak tut.
        let mut site = SiteDurumu::Yok;
        let mut site_linki = String::new();
        let mut gercek_site = false;
        for anahtar in WEB_SITESI_ETIKETLERI {
            let Some(deger) = etiket(tags, anahtar) else {
                continue;
            };
            match site_durumu_bul(deger) {
                Some(durum) => {
                    if site_linki.is_empty() {
                        site = durum;
                        site_linki = deger.to_string();
                    }
                }
                None => gercek_site = true,
            }
        }
        if gercek_site {
            siteli += 1;
            continue;
        }
        if site == SiteDurumu::Yok {
            // Sosyal medya bazen yalnızca ayrı etiketlerde bulunur.
            if let Some(link) = etiket(tags, "contact:instagram")
                .or_else(|| etiket(tags, "contact:facebook"))
                .or_else(|| etiket(tags, "instagram"))
                .or_else(|| etiket(tags, "facebook"))
            {
                site = SiteDurumu::Sosyal;
                site_linki = link.to_string();
            }
        }
        // Kaydın son düzenleme zamanı: eski kayıt = kapanmış olabilir.
        let son_guncelleme_gun = eleman
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|metin| chrono::DateTime::parse_from_rfc3339(metin).ok())
            .map(|tarih| (chrono::Local::now() - tarih.with_timezone(&chrono::Local)).num_days());

        let nokta = eleman.get("center").unwrap_or(eleman);
        let koordinat = match (
            nokta.get("lat").and_then(|v| v.as_f64()),
            nokta.get("lon").and_then(|v| v.as_f64()),
        ) {
            (Some(enlem), Some(boylam)) => Some((enlem, boylam)),
            _ => None,
        };
        let (enlem, boylam) = koordinat.unwrap_or((0.0, 0.0));
        if !gorulen.insert(format!("{ad}|{enlem:.4}|{boylam:.4}")) {
            continue;
        }

        let tur = eleman
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("node");
        let kimlik = eleman.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let telefon = etiket(tags, "phone")
            .or_else(|| etiket(tags, "contact:phone"))
            .unwrap_or("");

        isletmeler.push(BusinessLead {
            name: ad.to_string(),
            address: adres_metni(tags, ilce, il),
            category: sektor.to_string(),
            phone: telefon.to_string(),
            selected: true,
            kaynak: format!("https://www.openstreetmap.org/{tur}/{kimlik}"),
            konum: koordinat,
            calisma_saatleri: etiket(tags, "opening_hours").unwrap_or("").to_string(),
            kayit_dolulugu: kayit_dolulugu(tags),
            site,
            site_linki,
            son_guncelleme_gun,
            inceleme_sayisi: None,
            yildiz_puani: None,
            durum: musteriler::Durum::Yeni,
            demo_yolu: None,
            mesaj_hazir: false,
            yayin_linki: None,
            karne_linki: None,
            mesaj_varyanti: None,
        });
    }

    leads_sirala(&mut isletmeler);
    // İki küme ayrı kotalarla çekildiği için toplam sınır burada uygulanır:
    // en yüksek potansiyelli SONUC_SINIRI kayıt kalır.
    isletmeler.truncate(SONUC_SINIRI);
    Ok((isletmeler, siteli))
}

/// Overpass sorgusunu kurar. `hedef` "(area.a)" veya "(around:...)" olur;
/// web sitesi etiketi olan kayıtlar sunucu tarafında elenir ve sonuç sayısı
/// `SONUC_SINIRI` ile sınırlanır.
fn overpass_sorgusu(kurallar: &[(&str, &str)], hedef: &str, alan_id: Option<u64>) -> String {
    let mut govde = String::new();
    if let Some(alan_id) = alan_id {
        govde.push_str(&format!("area({alan_id})->.a;\n"));
    }
    // İki ayrı küme istenir:
    //  .sitesiz  : hiç web etiketi olmayanlar (klasik hedef kitle)
    //  .niyetli  : yalnızca sosyal medya veya ücretsiz site aracı kullananlar
    // Gerçek alan adı olan işletmeler sunucu tarafında istenmez. İki küme ayrı
    // `out` bloklarıyla ve kendi kotalarıyla çıkar; aksi hâlde niyetli grup,
    // dolu bir ilçede 20'lik genel sınıra takılıp sessizce kesilirdi.
    const SOSYAL_DESEN: &str = "instagram|facebook|fb\\.com|linktr|tiktok|wa\\.me|twitter|x\\.com";
    const UCRETSIZ_DESEN: &str = "business\\.site|wixsite|blogspot|wordpress\\.com|weebly|webnode";
    govde.push_str("(\n");
    for (anahtar, desen) in kurallar {
        govde.push_str(&format!(
            "  nwr[\"{anahtar}\"~\"{desen}\"][!\"website\"][!\"contact:website\"][!\"url\"][!\"contact:url\"]{hedef};\n"
        ));
    }
    govde.push_str(")->.sitesiz;\n(\n");
    for (anahtar, desen) in kurallar {
        for etiket in ["website", "contact:website"] {
            govde.push_str(&format!(
                "  nwr[\"{anahtar}\"~\"{desen}\"][\"{etiket}\"~\"{SOSYAL_DESEN}\",i]{hedef};\n"
            ));
            govde.push_str(&format!(
                "  nwr[\"{anahtar}\"~\"{desen}\"][\"{etiket}\"~\"{UCRETSIZ_DESEN}\",i]{hedef};\n"
            ));
        }
    }
    // meta: kaydın son düzenleme tarihi (tazelik) için gerekir.
    govde.push_str(&format!(
        ")->.niyetli;\n.sitesiz out center meta {SONUC_SINIRI};\n.niyetli out center meta {NIYETLI_KOTA};"
    ));
    format!("[out:json][timeout:60];\n{govde}")
}

/// Overpass sorgusu. Sunucular sırayla denenir: birincil sunucu 429/502/504
/// verdiğinde veya yanıt vermediğinde sıradaki sunucuya geçilir; böylece yoğun
/// saatte ilçe kaybolmaz. Dönen ikinci değer yanıtı veren sunucunun adıdır
/// (yedek kullanıldıysa günlüğe yazılır).
fn overpass_istek(ajan: &ureq::Agent, sorgu: &str) -> Result<(String, &'static str), String> {
    let mut gonder = |adres: &str| -> Result<String, String> {
        ajan
            // Sorgu ham gövde olarak gönderilir: `data=` biçimini bazı uç
            // noktalar (ör. z.overpass-api.de) reddediyor, ham gövde hepsinde
            // çalışıyor.
            .post(adres)
            .set("Content-Type", "text/plain; charset=utf-8")
            .send_string(sorgu)
            .map_err(http_hata)?
            .into_string()
            .map_err(|hata| format!("tarama yanıtı okunamadı: {hata}"))
    };
    overpass_istek_zincir(&mut gonder, &OVERPASS_SUNUCULARI, OVERPASS_BUTCE)
}

/// `overpass_istek`in sunucu zinciri mantığı: sunucular sırayla denenir, her
/// biri kendi deneme sayısıyla. Gönderim bir closure olarak alındığı için
/// zincir ağ olmadan da test edilebilir.
fn overpass_istek_zincir(
    gonder: &mut dyn FnMut(&str) -> Result<String, String>,
    sunucular: &[(&'static str, &'static str)],
    butce: Duration,
) -> Result<(String, &'static str), String> {
    let baslangic = Instant::now();
    let mut hatalar: Vec<String> = Vec::new();
    for &(adres, ad) in sunucular {
        if baslangic.elapsed() >= butce {
            hatalar.push(format!(
                "{} sn süre bütçesi doldu, kalan sunucular denenmedi",
                butce.as_secs()
            ));
            break;
        }
        match tekrar_dene_n(OVERPASS_DENEMESI, || gonder(adres)) {
            Ok(govde) => return Ok((govde, ad)),
            Err(hata) => hatalar.push(format!("{ad}: {hata}")),
        }
    }
    Err(format!(
        "Overpass sunucularının hiçbiri yanıt vermedi ({}).",
        hatalar.join(" · ")
    ))
}

fn sitesiz_isletmeler(
    ajan: &ureq::Agent,
    il: &str,
    ilce: &str,
    sektor: &str,
    konum: &Konum,
) -> Result<(Vec<BusinessLead>, usize, &'static str), String> {
    let kurallar = osm_kurallari(sektor);
    if kurallar.is_empty() {
        return Err(format!(
            "'{sektor}' sektörü için OSM eşlemesi tanımlı değil"
        ));
    }

    // 1) İlçenin gerçek idari sınır poligonu: komşu mahallelere taşmaz.
    if let Some(alan_id) = konum.alan_id {
        let sorgu = overpass_sorgusu(&kurallar, "(area.a)", Some(alan_id));
        let (govde, sunucu) = overpass_istek(ajan, &sorgu)?;
        let (liste, siteli) = overpass_isle(&govde, il, ilce, sektor)?;
        if !liste.is_empty() {
            return Ok((liste, siteli, sunucu));
        }
    }

    // 2) Sınır poligonu yoksa veya boş döndüyse merkez yarıçapına düş.
    let hedef = format!(
        "(around:{},{:.7},{:.7})",
        konum.yaricap_m, konum.enlem, konum.boylam
    );
    let sorgu = overpass_sorgusu(&kurallar, &hedef, None);
    let (govde, sunucu) = overpass_istek(ajan, &sorgu)?;
    let (liste, siteli) = overpass_isle(&govde, il, ilce, sektor)?;
    Ok((liste, siteli, sunucu))
}

/// Yanıt birincil sunucudan gelmediyse günlüğe yazar; aynı bilgi tekrar
/// edilmesin diye `bildirildi` bayrağı kullanılır.
fn yedek_sunucu_bildir(tx: &Sender<Mesaj>, ilce: &str, sunucu: &str, bildirildi: &mut bool) {
    let birincil = OVERPASS_SUNUCULARI[0].1;
    if sunucu == birincil || *bildirildi {
        return;
    }
    *bildirildi = true;
    let _ = tx.send(Mesaj::Log(format!(
        "[i] {ilce}: birincil Overpass sunucusu yanıt vermedi, yedek sunucu kullanıldı ({sunucu})."
    )));
}

/// İl geneli taramada sıradaki ilçeden alınacak kayıt sayısı: kalan kayıt
/// hakkı kalan ilçe sayısına eşit dağıtılır, böylece tek ilçe tüm listeyi
/// doldurmaz ve sonuç il geneline yayılır.
fn ilce_kotasi(kalan_kota: usize, kalan_ilce: usize) -> usize {
    if kalan_kota == 0 || kalan_ilce == 0 {
        return 0;
    }
    let pay = kalan_kota.div_ceil(kalan_ilce);
    pay.clamp(1, kalan_kota)
}

/// Tek ilçe taraması boş döndüyse il geneli taraması otomatik başlatılır;
/// il geneli de boşsa tekrar denenmez (aynı tarama döngüye girmesin).
fn il_geneline_gecilir(adet: usize, il_geneli: bool) -> bool {
    adet == 0 && !il_geneli
}

/// `ilce = None` ise ilin tüm ilçeleri sırayla taranır.
fn tarama_calistir(
    tx: Sender<Mesaj>,
    il: String,
    ilce: Option<String>,
    sektor: String,
    kaynak: Kaynak,
) {
    match ilce {
        Some(ilce) => tarama_tek_ilce(&tx, &il, &ilce, &sektor, kaynak),
        None => tarama_il_geneli(&tx, &il, &sektor, kaynak),
    }
}

/// İlin tüm ilçelerini sırayla tarar; sonuçlar `SONUC_SINIRI` ile sınırlıdır.
fn tarama_il_geneli(tx: &Sender<Mesaj>, il: &str, sektor: &str, kaynak: Kaynak) {
    let ilceler: Vec<&str> = iller::ILLER
        .iter()
        .find(|(ad, _)| *ad == il)
        .map(|(_, ilceler)| ilceler.to_vec())
        .unwrap_or_default();
    if ilceler.is_empty() {
        let _ = tx.send(Mesaj::Log(format!(
            "[!] {il} için ilçe listesi bulunamadı."
        )));
        let _ = tx.send(Mesaj::TaramaBasarisiz);
        return;
    }

    let ajan = http_ajani();
    let ayar = GoogleAyar::oku();
    let mut google_kota = kota_oku();
    let mut google_denemesi = 0usize;
    if kaynak == Kaynak::Google && !ayar.acik() {
        let _ = tx.send(Mesaj::Log(
            "[!] Google kaynağı seçili ama GOOGLE_PLACES_API_KEY tanımlı değil; .env dosyasına anahtar ekleyin veya OSM/Hibrit seçin."
                .to_string(),
        ));
        let _ = tx.send(Mesaj::TaramaBasarisiz);
        return;
    }
    let _ = tx.send(Mesaj::Log(format!(
        "[>] {il} il geneli: {} ilçe sırayla taranacak ({sektor}) · kaynak: {}.",
        ilceler.len(),
        kaynak.aciklama()
    )));

    let mut bulunanlar: Vec<BusinessLead> = Vec::new();
    let mut gorulen: HashSet<String> = HashSet::new();
    let mut siteli = 0usize;
    let mut atlanan = 0usize;
    let mut taranan = 0usize;
    // İlçe konumları yerel önbellekten gelirse Nominatim'e hiç gidilmez.
    let mut onbellek = KonumOnbellek::yukle();
    let mut onbellek_hazir = 0usize;
    let mut onbellek_yeni = 0usize;
    // İki turda da alınamayan ilçeler sonda tek satırda bildirilir; sessizce
    // kaybolmazlar.
    let mut kayip: Vec<String> = Vec::new();
    let mut yedek_bildirildi = false;
    // İlçe + sektör sonuç önbelleği: aynı ilçe tekrar taranırsa Overpass'a hiç
    // gidilmez.
    let mut sonuc_onbellek = SonucOnbellek::yukle();
    let mut onbellekten = 0usize;
    let mut agdan = 0usize;

    for (idx, ilce) in ilceler.iter().enumerate() {
        let kalan_kota = SONUC_SINIRI.saturating_sub(bulunanlar.len());
        if kalan_kota == 0 {
            break;
        }
        let kota = ilce_kotasi(kalan_kota, ilceler.len() - idx);
        taranan += 1;
        let _ = tx.send(Mesaj::Ilerleme {
            tamamlanan: idx,
            toplam: ilceler.len(),
        });
        let _ = tx.send(Mesaj::Log(format!(
            "[>] {ilce} / {il} taranıyor ({}/{}) · toplam {}/{SONUC_SINIRI} kayıt...",
            idx + 1,
            ilceler.len(),
            bulunanlar.len()
        )));

        // Google modunda koordinat gerekmez: Nominatim'e hiç gidilmez.
        if kaynak == Kaynak::Google {
            let (bugun, bu_ay) = (bugun_anahtari(), ay_anahtari());
            let gunluk_dolu = google_kota.gun == bugun && google_kota.gunluk >= ayar.gunluk_limit;
            let aylik_dolu = google_kota.ay == bu_ay && google_kota.aylik >= ayar.aylik_limit;
            if gunluk_dolu || aylik_dolu {
                let _ = tx.send(Mesaj::Log(format!(
                    "[!] Google bütçesi doldu (günlük {}/{}, aylık {}/{}); kalan {} ilçe taranmadı.",
                    google_kota.gunluk,
                    ayar.gunluk_limit,
                    google_kota.aylik,
                    ayar.aylik_limit,
                    ilceler.len() - idx
                )));
                break;
            }
            let oncesi = bulunanlar.len();
            match google_yedek(
                tx,
                &ajan,
                &ayar,
                &mut google_kota,
                il,
                ilce,
                sektor,
                "Google kaynağı seçili",
            ) {
                Some(liste) => {
                    for lead in liste {
                        if bulunanlar.len() >= SONUC_SINIRI {
                            break;
                        }
                        if gorulen.insert(lead.kaynak.clone()) {
                            bulunanlar.push(lead);
                        }
                    }
                }
                None => atlanan += 1,
            }
            let eklenen = bulunanlar.len() - oncesi;
            let _ = tx.send(Mesaj::Log(if eklenen == 0 {
                format!("[i] {ilce}: Google'da web sitesi olmayan kayıt yok.")
            } else {
                format!(
                    "[✓] {ilce}: {eklenen} kayıt eklendi (toplam {}/{SONUC_SINIRI}).",
                    bulunanlar.len()
                )
            }));
            thread::sleep(Duration::from_millis(1_100));
            continue;
        }

        let (konum, konum_arandi) = match onbellek.al(il, ilce) {
            Some(konum) => {
                onbellek_hazir += 1;
                (konum, false)
            }
            None => match konum_bul(&ajan, il, ilce) {
                Ok(konum) => {
                    onbellek.ekle(il, ilce, &konum);
                    onbellek.kaydet();
                    onbellek_yeni += 1;
                    (konum, true)
                }
                Err(hata) => {
                    atlanan += 1;
                    kayip.push(ilce.to_string());
                    let _ = tx.send(Mesaj::Log(format!("[!] {ilce} atlandı: {hata}")));
                    thread::sleep(Duration::from_millis(1_100));
                    continue;
                }
            },
        };

        // Sunucu zinciri tükendiğinde ilçe kaybolmasın: kısa bir beklemeden
        // sonra ikinci tur denenir.
        let mut tarama = ilce_taramasi(&ajan, &mut sonuc_onbellek, il, ilce, sektor, &konum);
        if let Some(hata) = tarama.hata() {
            let _ = tx.send(Mesaj::Log(format!(
                "[↻] {ilce} ilk turda alınamadı ({hata}); 4 sn sonra yeniden denenecek."
            )));
            thread::sleep(Duration::from_secs(4));
            tarama = ilce_taramasi(&ajan, &mut sonuc_onbellek, il, ilce, sektor, &konum);
        }
        match tarama.parcala() {
            Ok((mut liste, ilce_siteli, sunucu, onbellek_gunu)) => {
                match onbellek_gunu {
                    Some(gun) => {
                        onbellekten += 1;
                        let _ = tx.send(Mesaj::Log(format!(
                            "[i] {ilce}: sonuçlar önbellekten alındı ({} kayıt, {} gün önce); Overpass'a gidilmedi.",
                            liste.len(),
                            gun
                        )));
                    }
                    None => agdan += 1,
                }
                yedek_sunucu_bildir(tx, ilce, sunucu, &mut yedek_bildirildi);
                // İlçe OSM'de boşsa ve tarama bütçesi varsa Google'a düşülür;
                // böylece tek tarama tüm ücretsiz kotayı tüketmez.
                if kaynak == Kaynak::Hibrit
                    && liste.is_empty()
                    && google_denemesi < GOOGLE_TARAMA_SINIRI
                {
                    google_denemesi += 1;
                    if ayar.acik() {
                        if let Some(gelen) = google_yedek(
                            tx,
                            &ajan,
                            &ayar,
                            &mut google_kota,
                            il,
                            ilce,
                            sektor,
                            "OSM'de kayıt yok",
                        ) {
                            liste = gelen;
                        }
                    } else if google_denemesi == 1 {
                        let _ = tx.send(Mesaj::Log(
                            "[i] Google yedeği kapalı: GOOGLE_PLACES_API_KEY tanımlı değil."
                                .to_string(),
                        ));
                    }
                }
                siteli += ilce_siteli;
                let oncesi = bulunanlar.len();
                for lead in liste {
                    if bulunanlar.len() >= SONUC_SINIRI {
                        break;
                    }
                    if gorulen.insert(lead.kaynak.clone()) {
                        bulunanlar.push(lead);
                    }
                    if bulunanlar.len() - oncesi >= kota {
                        break;
                    }
                }
                let eklenen = bulunanlar.len() - oncesi;
                let _ = tx.send(Mesaj::Log(if eklenen == 0 {
                    format!("[i] {ilce}: web sitesi olmayan kayıt yok.")
                } else {
                    format!(
                        "[✓] {ilce}: {eklenen} kayıt eklendi (toplam {}/{SONUC_SINIRI}).",
                        bulunanlar.len()
                    )
                }));
            }
            Err(hata) => {
                atlanan += 1;
                kayip.push(ilce.to_string());
                let _ = tx.send(Mesaj::Log(format!(
                    "[!] {ilce} iki turda da alınamadı, listeye eklenmedi: {hata}"
                )));
            }
        }

        // Nominatim'e gidilmediyse bekleme kısaltılır: önbellekli ilçelerde il
        // geneli tarama hızlanır, Overpass yine yorulmaz.
        thread::sleep(Duration::from_millis(if konum_arandi {
            1_100
        } else {
            400
        }));
    }

    onbellek.kaydet();
    if onbellek_hazir > 0 || onbellek_yeni > 0 {
        let _ = tx.send(Mesaj::Log(format!(
            "[i] Konum önbelleği: {onbellek_hazir} ilçe hazır, {onbellek_yeni} yeni sorgu."
        )));
    }
    if onbellekten > 0 || agdan > 0 {
        let _ = tx.send(Mesaj::Log(format!(
            "[i] Sonuç önbelleği: {onbellekten} ilçe hazırdı (Overpass'a gidilmedi), {agdan} ilçe canlı tarandı · geçerlilik {} gün (SONUC_TTL_GUN).",
            sonuc_ttl_gun()
        )));
    }
    if !kayip.is_empty() {
        let _ = tx.send(Mesaj::Log(format!(
            "[!] {} ilçe iki turda da alınamadı: {}. Sunucu yoğunluğu geçtiğinde yalnızca bu ilçeleri seçip yeniden tarayın.",
            kayip.len(),
            kayip.join(", ")
        )));
    }
    let _ = tx.send(Mesaj::Ilerleme {
        tamamlanan: taranan,
        toplam: ilceler.len(),
    });
    let kalan_ilce = ilceler.len().saturating_sub(taranan);
    if kalan_ilce > 0 {
        let _ = tx.send(Mesaj::Log(format!(
            "[i] {SONUC_SINIRI} kayıt sınırına ulaşıldı; kalan {kalan_ilce} ilçe taranmadı."
        )));
    }
    // İlçe sırasına göre toplanan liste teslimattan önce sıralanır: Google
    // kaynağında inceleme sayısı, diğerlerinde satın alma potansiyeli.
    if kaynak == Kaynak::Google {
        incelemeye_gore_sirala(&mut bulunanlar);
    } else {
        leads_sirala(&mut bulunanlar);
    }
    let yuksek = bulunanlar
        .iter()
        .filter(|isletme| potansiyel_puani(isletme) >= SICAK_ESIK)
        .count();
    let _ = tx.send(Mesaj::Log(format!(
        "[✓] {il} il geneli bitti: {} ilçe tarandı, {} kayıt ({} sıcak fırsat), {} web sitesi olan kayıt ayıklandı{}.",
        taranan,
        bulunanlar.len(),
        yuksek,
        siteli,
        if atlanan == 0 {
            String::new()
        } else {
            format!(", {atlanan} ilçe atlandı")
        }
    )));
    let _ = tx.send(Mesaj::TaramaBitti(bulunanlar));
}

fn tarama_tek_ilce(tx: &Sender<Mesaj>, il: &str, ilce: &str, sektor: &str, kaynak: Kaynak) {
    let ajan = http_ajani();

    // Yalnız Google: koordinat gerekmez, Nominatim'e hiç gidilmez.
    if kaynak == Kaynak::Google {
        let ayar = GoogleAyar::oku();
        if !ayar.acik() {
            let _ = tx.send(Mesaj::Log(
                "[!] Google kaynağı seçili ama GOOGLE_PLACES_API_KEY tanımlı değil; .env dosyasına anahtar ekleyin veya OSM/Hibrit seçin."
                    .to_string(),
            ));
            let _ = tx.send(Mesaj::TaramaBasarisiz);
            return;
        }
        let mut kota = kota_oku();
        let _ = tx.send(Mesaj::Log(format!(
            "[>] Google Places: {il} / {ilce} {sektor} taranıyor (en fazla {SONUC_SINIRI} kayıt)..."
        )));
        match google_yedek(
            tx,
            &ajan,
            &ayar,
            &mut kota,
            il,
            ilce,
            sektor,
            "Google kaynağı seçili",
        ) {
            Some(liste) => {
                let yuksek = liste
                    .iter()
                    .filter(|isletme| potansiyel_puani(isletme) >= SICAK_ESIK)
                    .count();
                let _ = tx.send(Mesaj::Log(format!(
                    "[✓] {} potansiyel müşteri bulundu · {yuksek} sıcak fırsat.",
                    liste.len()
                )));
                let _ = tx.send(Mesaj::TaramaBitti(liste));
            }
            None => {
                let _ = tx.send(Mesaj::TaramaBasarisiz);
            }
        }
        return;
    }

    let mut onbellek = KonumOnbellek::yukle();
    let konum = match onbellek.al(il, ilce) {
        Some(konum) => {
            let _ = tx.send(Mesaj::Log(format!(
                "[i] {ilce} konumu önbellekten alındı (Konum araması yapılmadı)."
            )));
            konum
        }
        None => {
            let _ = tx.send(Mesaj::Log(format!("[>] {ilce} / {il} konumu aranıyor...")));
            match konum_bul(&ajan, il, ilce) {
                Ok(konum) => {
                    onbellek.ekle(il, ilce, &konum);
                    onbellek.kaydet();
                    konum
                }
                Err(hata) => {
                    let _ = tx.send(Mesaj::Log(format!("[!] Konum alınamadı: {hata}")));
                    let _ = tx.send(Mesaj::TaramaBasarisiz);
                    return;
                }
            }
        }
    };
    let alan = if konum.alan_id.is_some() {
        "ilçe sınırı"
    } else {
        "merkez yarıçapı"
    };
    let _ = tx.send(Mesaj::Log(format!(
        "[>] OpenStreetMap: {il} / {ilce} {sektor} taranıyor ({alan}, en fazla {SONUC_SINIRI} kayıt)..."
    )));
    let mut yedek_bildirildi = false;
    let mut sonuc_onbellek = SonucOnbellek::yukle();
    let tarama = ilce_taramasi(&ajan, &mut sonuc_onbellek, il, ilce, sektor, &konum);
    match tarama.parcala() {
        Ok((mut liste, siteli, sunucu, onbellek_gunu)) => {
            if let Some(gun) = onbellek_gunu {
                let _ = tx.send(Mesaj::Log(format!(
                    "[i] {ilce}: sonuçlar önbellekten alındı ({} kayıt, {} gün önce); Overpass'a gidilmedi.",
                    liste.len(),
                    gun
                )));
            }
            yedek_sunucu_bildir(tx, ilce, sunucu, &mut yedek_bildirildi);
            // Hibritte OSM boş dönerse ve bütçe varsa Google yedeği devreye girer.
            if kaynak == Kaynak::Hibrit && liste.is_empty() {
                let ayar = GoogleAyar::oku();
                if ayar.acik() {
                    let mut kota = kota_oku();
                    if let Some(gelen) = google_yedek(
                        tx,
                        &ajan,
                        &ayar,
                        &mut kota,
                        il,
                        ilce,
                        sektor,
                        "OSM'de kayıt yok",
                    ) {
                        liste = gelen;
                    }
                } else {
                    let _ = tx.send(Mesaj::Log(
                        "[i] Google yedeği kapalı: GOOGLE_PLACES_API_KEY tanımlı değil, yalnızca OSM kullanılıyor."
                            .to_string(),
                    ));
                }
            }
            let yuksek = liste
                .iter()
                .filter(|isletme| potansiyel_puani(isletme) >= SICAK_ESIK)
                .count();
            let ozet = if siteli == 0 {
                format!(
                    "[✓] {} potansiyel müşteri bulundu · {yuksek} sıcak fırsat.",
                    liste.len()
                )
            } else {
                format!(
                    "[✓] {} potansiyel müşteri bulundu ({} sitesi olan kayıt ayıklandı) · {yuksek} sıcak fırsat.",
                    liste.len(),
                    siteli
                )
            };
            let _ = tx.send(Mesaj::Log(ozet));
            let _ = tx.send(Mesaj::TaramaBitti(liste));
        }
        Err(hata) => {
            let _ = tx.send(Mesaj::Log(format!("[!] Tarama başarısız: {hata}")));
            let _ = tx.send(Mesaj::TaramaBasarisiz);
        }
    }
}

// ---------------------------------------------------------------------------
// Demo üretimi ve WhatsApp (wa.me) hazırlığı
//
// WhatsApp'ta toplu otomatik gönderim, resmî olmayan otomasyon kullandığı için
// numarayı askıya aldırır. Burada ücretsiz ve güvenli yol kullanılır: her
// işletme için gerçek bir HTML demo yazılır, wa.me bağlantısı mesaj hazır
// şekilde açılır; göndermeyi kullanıcı onaylar.
// ---------------------------------------------------------------------------

const DEMO_KLASORU: &str = "demos";
/// Sorgu başına listeye giren en fazla kayıt sayısı.
const SONUC_SINIRI: usize = 20;
/// Toplam 20 kaydın kaçı "yalnızca sosyal medyası olan" işletmelere ayrılır.
/// Bu grup en sıcak hedef kitle olduğu için kotası garantidir.
const NIYETLI_KOTA: usize = 8;

fn html_kacis(metin: &str) -> String {
    metin
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn dosya_slug(metin: &str) -> String {
    let mut ham = String::new();
    for karakter in metin.chars() {
        let kucuk = karakter.to_lowercase().next().unwrap_or(karakter);
        ham.push(match kucuk {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            'â' => 'a',
            'î' => 'i',
            'û' => 'u',
            'a'..='z' | '0'..='9' => kucuk,
            _ => '-',
        });
    }
    let slug = ham
        .split('-')
        .filter(|parca| !parca.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "isletme".to_string()
    } else {
        slug.chars().take(60).collect()
    }
}

/// "+90 532 123 45 67", "0532..." ve "532..." biçimlerini wa.me numarasına çevirir.
fn whatsapp_numarasi(ham: &str) -> Option<String> {
    let rakamlar: String = ham
        .chars()
        .filter(|karakter| karakter.is_ascii_digit())
        .collect();
    if rakamlar.is_empty() {
        return None;
    }
    let numara = if let Some(kalan) = rakamlar.strip_prefix("00") {
        kalan.to_string()
    } else if rakamlar.starts_with("90") && rakamlar.len() == 12 {
        rakamlar
    } else if rakamlar.starts_with('0') && rakamlar.len() == 11 {
        format!("90{}", &rakamlar[1..])
    } else if rakamlar.len() == 10 {
        format!("90{rakamlar}")
    } else {
        rakamlar
    };
    (10..=15).contains(&numara.len()).then_some(numara)
}

/// Sektörün web sitesine duyduğu ihtiyaç: müşterisi internette arayan, menü /
/// hizmet / fiyat karşılaştıran sektörler demo satın almaya daha yakındır.
/// Ağırlıklar 5-20 arasıdır; tek başına belirleyici değildir, ulaşılabilirliğin
/// (telefon) arkasından gelir.
fn sektor_potansiyeli(sektor: &str) -> u8 {
    match demo_turu(sektor) {
        // Menü, hizmet listesi veya rezervasyon doğrudan satışa dönüşür.
        Demo::Menu | Demo::Klinik | Demo::Otel => 20,
        // Görsel vitrin, program ve referans ihtiyacı yüksek.
        Demo::Galeri | Demo::Egitim | Demo::Ofis => 15,
        Demo::Kafe | Demo::Unlu | Demo::Servis | Demo::Eglence => 10,
        // Ürünleri pazaryerlerinde de satıldığı için site ihtiyacı görece düşük.
        Demo::Urun => 5,
    }
}

/// Bu puanın üstündeki işletmeler arayüzde "sıcak fırsat" işaretlenir ve
/// listenin en başında gösterilir.
const SICAK_ESIK: u8 = 75;

/// İşletmenin satın alma niyeti puanı (0-100). Yalnızca tarama sırasında
/// elimizde olan veriler kullanılır; ek ağ isteği yapılmaz. Ağırlıklar niyet
/// sırasına göre değil, "site yaptırmaya ne kadar yakın" sırasına göre verilir:
///
///   web varlığı: sosyal medya var ama site yok  +38
///                ücretsiz site aracı            +28
///                hiçbir web varlığı yok         +12
///   (niyet, ulaşılabilirlikten daha ağır basar: çevrimiçi görünmek isteyip
///    site yaptırmamış işletme, sitesiz ve iletişimsiz işletmeden öndedir.)
///   telefon (mobil numara ayrıca +5)            +25
///   Google inceleme sayısı (gerçek müşteri)     +0..18
///   çalışma saatleri (aktif işletme)            +8
///   adres                                        +5..8
///   kaydın son 12/36 ayda güncellenmiş olması    +2..5
///   sektörün web ihtiyacı                        +5..20
///   koordinat                                    +3
///   kayıt doluluğu                               +0..5
fn potansiyel_puani(isletme: &BusinessLead) -> u8 {
    let mut puan: u16 = match isletme.site {
        // Çevrimiçi görünmek istediğini göstermiş ama site yaptırmamış.
        SiteDurumu::Sosyal => 38,
        // Site ihtiyacını kabul etmiş, ücretsiz araçla idare ediyor.
        SiteDurumu::Ucretsiz => 28,
        // Henüz web ihtiyacını fark etmemiş olabilir.
        SiteDurumu::Yok => 12,
    };
    if let Some(numara) = whatsapp_numarasi(&isletme.phone) {
        puan += 25;
        // Mobil numara genellikle işletme sahibinin kendi telefonudur.
        if numara.starts_with("905") {
            puan += 5;
        }
    }
    if let Some(inceleme) = isletme.inceleme_sayisi {
        puan += match inceleme {
            100.. => 18,
            30..=99 => 12,
            10..=29 => 6,
            _ => 0,
        };
    }
    if !isletme.calisma_saatleri.trim().is_empty() {
        puan += 8;
    }
    if isletme.address.contains("No:") {
        puan += 8;
    } else if isletme.address.contains(',') {
        puan += 5;
    }
    if let Some(gun) = isletme.son_guncelleme_gun {
        puan += if gun <= 365 {
            5
        } else if gun <= 1_095 {
            2
        } else {
            0
        };
    }
    puan += sektor_potansiyeli(&isletme.category) as u16;
    if isletme.konum.is_some() {
        puan += 3;
    }
    puan += (isletme.kayit_dolulugu.min(10) / 2) as u16;
    puan.min(100) as u8
}

/// Puanın nereden geldiğini tek satırda açıklar:
/// "86/100 · sosyal medya var, site yok · mobil numara · ...".
/// Arayüzde gerekçe olarak gösterilir; böylece sıralama şeffaf kalır.
fn potansiyel_gerekcesi(isletme: &BusinessLead) -> String {
    let mut parcalar: Vec<String> = Vec::new();
    match isletme.site {
        SiteDurumu::Sosyal => parcalar.push("sosyal medya var, site yok".to_string()),
        SiteDurumu::Ucretsiz => parcalar.push("ücretsiz site aracı".to_string()),
        SiteDurumu::Yok => {}
    }
    if let Some(numara) = whatsapp_numarasi(&isletme.phone) {
        parcalar.push(if numara.starts_with("905") {
            "mobil numara".to_string()
        } else {
            "sabit numara".to_string()
        });
    }
    if let Some(inceleme) = isletme.inceleme_sayisi {
        if inceleme >= 10 {
            parcalar.push(format!("{inceleme} Google incelemesi"));
        }
    }
    if !isletme.calisma_saatleri.trim().is_empty() {
        parcalar.push("çalışma saatleri".to_string());
    }
    if isletme.address.contains("No:") {
        parcalar.push("kapı numaralı adres".to_string());
    } else if isletme.address.contains(',') {
        parcalar.push("sokak/mahalle".to_string());
    }
    if let Some(gun) = isletme.son_guncelleme_gun {
        if gun <= 365 {
            parcalar.push("güncel kayıt".to_string());
        }
    }
    match sektor_potansiyeli(&isletme.category) {
        20 => parcalar.push("sektör ihtiyacı yüksek".to_string()),
        15 => parcalar.push("sektör ihtiyacı orta".to_string()),
        _ => {}
    }
    if parcalar.is_empty() {
        parcalar.push("yalnızca isim".to_string());
    }
    format!(
        "{}/100 · {}",
        potansiyel_puani(isletme),
        parcalar.join(" · ")
    )
}

/// Arama sonuçlarını satın alma potansiyeline göre sıralar: puanı yüksek olan
/// önce, eşitlikte alfabetik sıra (Türkçe karakterler sadeleştirilerek). Sorgu
/// başına 20 kayıt sınırı olduğu için listeye hangi kayıtların girdiği de bu
/// sıralamaya bağlıdır; il geneli taramada ilçe başına ayrılan kota da böylece
/// ilçenin en yüksek potansiyelli kayıtlarından doldurulur.
/// Google kaynağında sıralama inceleme sayısına göredir: gerçek müşteri
/// trafiği olan işletme önce gelir (Google'ın kendi sonuç sırası bölgeye göre
/// değişebiliyor). Eşitlikte potansiyel puanı, sonra ad.
fn incelemeye_gore_sirala(liste: &mut [BusinessLead]) {
    liste.sort_by(|a, b| {
        let inceleme = |lead: &BusinessLead| lead.inceleme_sayisi.unwrap_or(0);
        inceleme(b)
            .cmp(&inceleme(a))
            .then_with(|| potansiyel_puani(b).cmp(&potansiyel_puani(a)))
            .then_with(|| arama_anahtari(&a.name).cmp(&arama_anahtari(&b.name)))
    });
}

fn leads_sirala(liste: &mut [BusinessLead]) {
    liste.sort_by(|a, b| {
        // Kara listedeki işletme ("reddetti") hiçbir zaman listenin başında
        // görünmez; kalanlar puana, eşit puanda iletişim durumuna göre sıralanır.
        a.durum
            .kara_listede()
            .cmp(&b.durum.kara_listede())
            .then_with(|| potansiyel_puani(b).cmp(&potansiyel_puani(a)))
            .then_with(|| a.durum.sira().cmp(&b.durum.sira()))
            .then_with(|| arama_anahtari(&a.name).cmp(&arama_anahtari(&b.name)))
    });
}

const GUN_ADLARI: [&str; 7] = [
    "Pazartesi",
    "Salı",
    "Çarşamba",
    "Perşembe",
    "Cuma",
    "Cumartesi",
    "Pazar",
];

fn saat_metni(dakika: u32) -> String {
    format!("{:02}:{:02}", dakika / 60, dakika % 60)
}

/// Verilen gün/saat için "açık / kapalı + açılış" cümlesi üretir.
fn durum_cumlesi(program: &[Vec<(u32, u32)>; 7], gun: usize, dakika: u32) -> String {
    for (bas, son) in &program[gun] {
        if dakika >= *bas && dakika < *son {
            if *bas == 0 && *son >= 1440 {
                return "İşletmeniz şu an açık (24 saat).".to_string();
            }
            if *son >= 1440 {
                return "İşletmeniz şu an açık (gece yarısına kadar).".to_string();
            }
            return format!("İşletmeniz şu an açık (kapanış {}).", saat_metni(*son));
        }
    }

    for i in 0..7 {
        let hedef = (gun + i) % 7;
        for (bas, _) in &program[hedef] {
            if i == 0 && *bas <= dakika {
                continue;
            }
            let etiket = match i {
                0 => "bugün",
                1 => "yarın",
                _ => GUN_ADLARI[hedef],
            };
            return format!(
                "İşletmeniz şu an kapalı; açılış {} {}.",
                etiket,
                saat_metni(*bas)
            );
        }
    }
    "İşletmeniz şu an kapalı.".to_string()
}

fn whatsapp_durumu(isletme: &BusinessLead) -> Option<String> {
    let program = haftalik_program(&isletme.calisma_saatleri)?;
    let simdi = chrono::Local::now();
    Some(durum_cumlesi(
        &program,
        simdi.weekday().num_days_from_monday() as usize,
        simdi.hour() * 60 + simdi.minute(),
    ))
}

/// Panelde gösterilecek kısa durum: "Şu an açık (kapanış 18:00)".
fn durum_kisa_cumle(cumle: &str) -> String {
    let temiz = cumle
        .trim_start_matches("İşletmeniz ")
        .trim_end_matches('.');
    let mut karakterler = temiz.chars();
    match karakterler.next() {
        Some(ilk) => ilk.to_uppercase().collect::<String>() + karakterler.as_str(),
        None => String::new(),
    }
}

/// Hazır mesaj: işletmenin defterdeki A/B varyantı kullanılır.
fn whatsapp_mesaji(isletme: &BusinessLead) -> String {
    let durum = whatsapp_durumu(isletme);
    whatsapp_mesaji_varyant(
        isletme,
        durum.as_deref(),
        isletme.mesaj_varyanti.unwrap_or_default(),
    )
}

/// Mesajdaki sektör cümlesi: demoda gerçekten hangi bölüm üretiliyorsa onu
/// anlatır (`demo_turu` ile aynı eşleme), böylece vaat ile sayfa tutarlı kalır.
fn sektor_demo_notu(sektor: &str) -> &'static str {
    match demo_turu(sektor) {
        // Hepsi 3-4 kelime: tanıtım cümlesi mesajı uzatmamalı.
        Demo::Menu => "menü ve günün yemekleri",
        Demo::Kafe => "menü ve tatlılar",
        Demo::Unlu => "ürünler ve özel sipariş",
        Demo::Urun => "ürün vitrini ve teslimat",
        // Kuaför/berber/güzellik: sayfadaki galeri ve tek tıkla arama.
        Demo::Galeri => "fotoğraf galerisi ve randevu",
        // Klinik: hizmet kartlarında uzman kadro vurgusu var.
        Demo::Klinik => "hizmetler ve uzman kadro",
        Demo::Otel => "odalar ve olanaklar",
        Demo::Egitim => "kurslar ve deneme dersi",
        Demo::Ofis => "hizmetler ve teklif",
        Demo::Servis => "hizmetler ve fiyat teklifi",
        Demo::Eglence => "program ve rezervasyon",
    }
}

/// Mesaja giren somut gerekçe: işletmenin kendi kaydından (Google yorum/puan,
/// web varlığı, çalışma saati). Uydurma cümle kurulmaz; elde veri yoksa o
/// kısım hiç yazılmaz.
/// Parçanın sokak/cadde olduğunu söyler: "No:12" ya da "Cad.", "Sokak", "Bulvarı"
/// gibi kelimeler. Mahalle adları ("Caddebostan") yanlışlıkla eşleşmesin diye
/// kelime sınırına bakılır.
fn sokak_mi(parca: &str) -> bool {
    let kucuk = parca.to_lowercase();
    if kucuk.contains("no:") {
        return true;
    }
    kucuk.split(|k: char| !k.is_alphabetic()).any(|kelime| {
        matches!(
            kelime,
            "cad"
                | "cadde"
                | "caddesi"
                | "cd"
                | "sok"
                | "sokak"
                | "sokagi"
                | "sokağı"
                | "bulvar"
                | "bulvari"
                | "bulvarı"
                | "blv"
                | "meydan"
                | "meydani"
                | "meydanı"
        )
    })
}

/// Mesajda tam sokak adresi yerine yalnızca mahalle/ilçe kalır:
/// "Moda Cad. No:12, Caferağa, Kadıköy / İstanbul" → "Caferağa, Kadıköy / İstanbul".
/// Mahalle yoksa yalnızca "Kadıköy / İstanbul" yazılır; adres zaten kısaysa
/// olduğu gibi döner.
fn kisa_adres(adres: &str) -> String {
    let parcalar: Vec<&str> = adres
        .split(',')
        .map(str::trim)
        .filter(|parca| !parca.is_empty())
        .collect();
    if parcalar.len() < 2 {
        return adres.trim().to_string();
    }
    // Son parça her zaman "İlçe / İl", ondan önceki mahalle ya da cadde.
    let yer = parcalar[parcalar.len() - 1];
    let onceki = parcalar[parcalar.len() - 2];
    if sokak_mi(onceki) {
        yer.to_string()
    } else {
        format!("{onceki}, {yer}")
    }
}

fn whatsapp_gerekcesi(isletme: &BusinessLead) -> Option<String> {
    let itibar = match (isletme.inceleme_sayisi, isletme.yildiz_puani) {
        (Some(adet), Some(puan)) if adet > 0 => Some(format!(
            "Google'da {}★ ve {adet} yorumunuz var",
            puan_metni(puan)
        )),
        (Some(adet), _) if adet > 0 => Some(format!("Google'da {adet} yorumunuz var")),
        (_, Some(puan)) => Some(format!("Google'da {}★ puanınız var", puan_metni(puan))),
        _ => None,
    };
    // Kısa ve tek başına da okunabilen ifadeler: itibar yoksa cümle bununla başlar.
    let site = match isletme.site {
        SiteDurumu::Yok => "web siteniz yok",
        SiteDurumu::Sosyal => "sosyal medya dışında siteniz yok",
        SiteDurumu::Ucretsiz => "kendi alan adınızda siteniz yok",
    };
    let mut cumle = match itibar {
        // Yorum/puan varsa "bu kadar görünürsünüz ama…" kurgusu en güçlüsü.
        Some(itibar) => format!("{itibar} ama {site}"),
        None => {
            let mut harfler = site.chars();
            match harfler.next() {
                Some(ilk) => ilk.to_uppercase().collect::<String>() + harfler.as_str(),
                None => String::new(),
            }
        }
    };
    if isletme.calisma_saatleri.trim().is_empty() {
        cumle.push_str(", çalışma saatiniz de yok");
    }
    Some(format!("{cumle}."))
}

fn whatsapp_mesaji_varyant(
    isletme: &BusinessLead,
    durum: Option<&str>,
    varyant: musteriler::Varyant,
) -> String {
    let ek = match durum {
        Some(durum) => format!(" {durum}"),
        None => String::new(),
    };
    // Demo tanıtımı sektör başına birkaç kelime; harita ve açık/kapalı bilgisi
    // zaten hemen önceki durum cümlesinde ve sayfada yer alıyor.
    let demo_notu = format!("demoda {}", sektor_demo_notu(&isletme.category));
    // Ticari ileti kuralları gereği: kaynağı belirt ve aynı kanaldan ret hakkı sun.
    let kaynak = if isletme.kaynak.contains("google.com") {
        "Google"
    } else {
        "OpenStreetMap"
    };
    // Somut gerekçe ilk yanıtın parçası: "neden yazıyorum"un cevabı.
    let gerekce = match whatsapp_gerekcesi(isletme) {
        Some(cumle) => format!(" {cumle}"),
        None => String::new(),
    };
    let adres = kisa_adres(&isletme.address);
    // Kapanış iki varyantta ortak; yalnızca açılış ve taslak cümlesi değişir.
    let kapanis = format!(
        " Beğenirseniz alan adınızda yayına alabilirim. Numaraya {kaynak} kaydınızdan ulaştım; istemezseniz \"RET\" yazın."
    );
    match varyant {
        // A: soruyla açılır, taslak kısa bir etiketle sunulur.
        musteriler::Varyant::A => {
            let taslak = match isletme.yayin_linki.as_deref() {
                Some(link) => format!(" Taslak: {link} — {demo_notu}."),
                None => format!(" 12 saatlik demo dosyasını ekliyorum; {demo_notu}."),
            };
            format!(
                "Merhaba, {} ({adres}) için hazırladığım web sitesi taslağını incelemek ister misiniz?{gerekce}{ek}{taslak}{kapanis}",
                isletme.name
            )
        }
        // B: soru sormadan doğrudan eksikleri söyler, taslağı çözüm olarak verir.
        musteriler::Varyant::B => {
            let taslak = match isletme.yayin_linki.as_deref() {
                Some(link) => {
                    format!(" Eksikleri kapatan kısa bir web sitesi taslağı hazırladım: {link} — {demo_notu}.")
                }
                None => format!(
                    " Eksikleri kapatan kısa bir taslak hazırladım; 12 saatlik demo dosyasını ekliyorum — {demo_notu}."
                ),
            };
            format!(
                "Merhaba, {} ({adres}).{gerekce}{ek}{taslak}{kapanis}",
                isletme.name
            )
        }
    }
}

fn hizmet_kartlari(baslik: &str, kartlar: &[(&str, &str, &str)]) -> String {
    let mut govde = String::new();
    for (ikon, ad, aciklama) in kartlar {
        govde.push_str(&format!(
            r##"<div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
<div class="text-xl mb-2">{ikon}</div>
<div class="font-semibold mb-1">{ad}</div>
<p class="text-sm text-zinc-400">{aciklama}</p>
</div>
"##
        ));
    }
    format!(
        r##"<section class="mt-14">
<h2 class="text-2xl font-bold mb-6">{baslik}</h2>
<div class="grid gap-4 sm:grid-cols-2">
{govde}</div>
</section>
"##
    )
}

fn menu_bolumu() -> String {
    let kalemler = [
        ("Günün Çorbası", "Mevsim sebzeleriyle, her gün taze"),
        ("Izgara Ana Yemek", "Günlük hazırlanan ana yemekler"),
        ("Ev Yapımı Tatlı", "Günün tatlısı, sıcak servis"),
        ("Türk Kahvesi", "Yanında ikramla"),
    ];
    let mut govde = String::new();
    for (ad, aciklama) in kalemler {
        govde.push_str(&format!(
            r##"<div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
<div class="font-semibold">{ad}</div>
<div class="text-xs text-zinc-500 mt-1">{aciklama}</div>
</div>
"##
        ));
    }
    format!(
        r##"<section class="mt-14">
<h2 class="text-2xl font-bold mb-2">Menü</h2>
<p class="text-zinc-500 text-sm mb-6">Örnek menü — gerçek menünüzle değiştirilecek.</p>
<div class="grid gap-3 sm:grid-cols-2">
{govde}</div>
</section>
"##
    )
}

fn galeri_bolumu() -> String {
    let kutular = [
        ("Saç Kesimi", "from-amber-500/40"),
        ("Boya & Röfle", "from-rose-500/40"),
        ("Bakım", "from-emerald-500/40"),
        ("Gelin Saçı", "from-sky-500/40"),
        ("Makyaj", "from-fuchsia-500/40"),
        ("Tırnak", "from-orange-500/40"),
    ];
    let mut govde = String::new();
    for (etiket, renk) in kutular {
        govde.push_str(&format!(
            r##"<div class="aspect-square rounded-xl bg-gradient-to-br {renk} to-zinc-900 border border-zinc-800 flex items-end p-3 text-sm font-medium">{etiket}</div>
"##
        ));
    }
    format!(
        r##"<section class="mt-14">
<h2 class="text-2xl font-bold mb-2">Galeri</h2>
<p class="text-zinc-500 text-sm mb-6">Örnek görseller — kendi fotoğraflarınızla değiştirilecek.</p>
<div class="grid grid-cols-2 sm:grid-cols-3 gap-3">
{govde}</div>
</section>
"##
    )
}

/// İşletmenin sektörüne göre demo sayfasına eklenen bölüm.
fn demo_bolumu(sektor: &str) -> String {
    match demo_turu(sektor) {
        Demo::Menu => menu_bolumu(),
        Demo::Kafe => kafe_bolumu(),
        Demo::Unlu => unlu_bolumu(),
        Demo::Urun => urun_bolumu(),
        Demo::Galeri => galeri_bolumu(),
        Demo::Klinik => klinik_bolumu(),
        Demo::Otel => otel_bolumu(),
        Demo::Egitim => egitim_bolumu(),
        Demo::Ofis => ofis_bolumu(sektor),
        Demo::Servis => servis_bolumu(sektor),
        Demo::Eglence => eglence_bolumu(sektor),
    }
}

fn kafe_bolumu() -> String {
    hizmet_kartlari(
        "Menü",
        &[
            (
                "☕",
                "Kahveler",
                "Filtre kahve, Türk kahvesi ve soğuk seçenekler.",
            ),
            (
                "🍰",
                "Tatlılar",
                "Günlük hazırlanan pasta ve tatlı çeşitleri.",
            ),
            ("🥐", "Atıştırmalıklar", "Sandviç, tost ve kahvaltı tabağı."),
        ],
    )
}

fn unlu_bolumu() -> String {
    hizmet_kartlari(
        "Ürünlerimiz",
        &[
            (
                "🥖",
                "Taze Üretim",
                "Her gün taze çıkan ekmek ve hamur işleri.",
            ),
            (
                "🎂",
                "Özel Sipariş",
                "Doğum günü ve davetler için özel pasta.",
            ),
            ("🤍", "Yanında", "Kahve, çay ve günlük atıştırmalıklar."),
        ],
    )
}

fn urun_bolumu() -> String {
    hizmet_kartlari(
        "Ürünler",
        &[
            (
                "🆕",
                "Yeni Gelenler",
                "Sezonun yeni ürünleri ve kampanyalar.",
            ),
            (
                "⭐",
                "Çok Satanlar",
                "Müşterilerin en çok tercih ettiği ürünler.",
            ),
            ("🚚", "Teslimat", "Mağazadan teslim veya adrese gönderim."),
        ],
    )
}

fn klinik_bolumu() -> String {
    hizmet_kartlari(
        "Hizmetlerimiz",
        &[
            (
                "🩺",
                "Muayene ve Kontrol",
                "Durum değerlendirmesi ve tedavi planı.",
            ),
            ("✨", "Tedavi", "Uzman kadroyla planlı ve takipli tedavi."),
            ("📅", "Randevu", "Uygun saate hızlı randevu."),
        ],
    )
}

fn otel_bolumu() -> String {
    hizmet_kartlari(
        "Odalar ve Konaklama",
        &[
            (
                "🛏️",
                "Standart Oda",
                "Konforlu yatak, klima ve ücretsiz Wi-Fi.",
            ),
            ("🌅", "Deluxe Oda", "Geniş oda, manzara ve kahvaltı dahil."),
            (
                "☕",
                "Kahvaltı & Olanaklar",
                "Açık büfe kahvaltı, otopark, resepsiyon.",
            ),
        ],
    )
}

fn egitim_bolumu() -> String {
    hizmet_kartlari(
        "Kurslar",
        &[
            ("🎯", "Bireysel Ders", "Kişiye özel program."),
            ("👥", "Grup Sınıfı", "Küçük gruplarla düzenli ders."),
            ("🆓", "Deneme Dersi", "Ücretsiz tanışma dersi."),
        ],
    )
}

/// Profesyonel hizmetler: sektöre göre hazırlanmış üç kart.
fn ofis_bolumu(sektor: &str) -> String {
    let kartlar: &[(&str, &str, &str)] = match sektor {
        "Mimarlık" => &[
            (
                "📐",
                "Mimari Proje",
                "Konsept, ruhsat ve uygulama projeleri.",
            ),
            ("🛋️", "İç Mimarlık", "Mekân tasarımı ve malzeme seçimi."),
            (
                "🧱",
                "Tadilat & Uygulama",
                "Anahtar teslim tadilat ve denetim.",
            ),
        ],
        "Emlak" => &[
            ("🏠", "Satılık & Kiralık", "Bölgedeki güncel portföy."),
            ("📊", "Değerleme", "Mülkünüz için gerçekçi fiyat analizi."),
            (
                "📝",
                "Tapu & Sözleşme",
                "Süreç boyunca resmî işlem desteği.",
            ),
        ],
        "Muhasebe" => &[
            (
                "🧾",
                "Vergi & Beyanname",
                "Aylık ve yıllık beyanname takibi.",
            ),
            ("🏢", "Şirket Kuruluşu", "Kuruluş işlemleri ve açılış."),
            (
                "👥",
                "Bordro & Danışmanlık",
                "Personel ve mali danışmanlık.",
            ),
        ],
        "Nakliyat" => &[
            ("🚚", "Şehir İçi Taşıma", "Ev ve ofis taşımacılığı."),
            ("🛣️", "Şehirler Arası", "Planlı ve sigortalı nakliye."),
            ("📦", "Paketleme", "Eşyalarınız için güvenli paketleme."),
        ],
        "Yazılım" => &[
            ("💻", "Web & Mobil", "İşletmenize özel uygulamalar."),
            ("⚡", "Otomasyon", "Tekrarlayan işleri otomatikleştirme."),
            ("🛡️", "Bakım & Destek", "Güncelleme, yedekleme ve destek."),
        ],
        _ => &[
            (
                "📋",
                "Danışmanlık",
                "İhtiyacınıza uygun çözüm ve süreç planı.",
            ),
            (
                "🧮",
                "Teklif & Planlama",
                "Şeffaf fiyat, net takvim ve kapsam.",
            ),
            ("🤝", "Takip & Destek", "İş bitiminde de yanınızdayız."),
        ],
    };
    hizmet_kartlari("Hizmetlerimiz", kartlar)
}

/// Tamir, bakım ve teknik servis sektörleri.
fn servis_bolumu(sektor: &str) -> String {
    let kartlar: &[(&str, &str, &str)] = match sektor {
        "Oto Tamir" | "Lastikçi" | "Oto Yıkama" | "Oto Kiralama" => &[
            (
                "🛠️",
                "Bakım & Onarım",
                "Periyodik bakım, fren ve süspansiyon.",
            ),
            (
                "🛞",
                "Lastik & Balans",
                "Lastik değişimi, balans ve rot ayarı.",
            ),
            ("📅", "Randevu", "Sıra beklemeden teslim ve teslim alma."),
        ],
        "Tesisatçı" | "Elektrikçi" | "Klima" | "Boyacı" | "Marangoz" | "Camcı" | "Çilingir" => {
            &[
                ("🔧", "Keşif & Teklif", "Yerinde inceleme, net fiyat."),
                ("🏠", "Uygulama", "Temiz çalışma, planlanan günde teslim."),
                ("🛡️", "Garanti", "İşçilik garantisi ve acil destek hattı."),
            ]
        }
        "Terzi" | "Kuru Temizleme" | "Ayakkabı Tamiri" => &[
            ("✂️", "Özel İşçilik", "Ölçüye ve isteğe göre düzenleme."),
            ("⏱️", "Hızlı Teslim", "Aynı gün teslim seçeneği."),
            (
                "🧺",
                "Bakım & Paketleme",
                "Temiz, ütülü ve korumalı teslim.",
            ),
        ],
        "Matbaa" => &[
            ("🖨️", "Dijital Baskı", "Kartvizit, broşür ve afiş."),
            ("🎨", "Tasarım", "Dosya hazırlama ve baskı öncesi kontrol."),
            ("🚚", "Teslim", "Şehir içi hızlı teslimat."),
        ],
        _ => &[
            ("🔧", "Bakım", "Periyodik kontrol ve düzenli bakım."),
            ("⚙️", "Onarım", "Arıza tespiti ve kalıcı çözüm."),
            ("📞", "Destek", "Telefonla ön bilgi ve yerinde servis."),
        ],
    };
    hizmet_kartlari("Servis Hizmetleri", kartlar)
}

/// Spor, eğlence ve etkinlik mekânları.
fn eglence_bolumu(sektor: &str) -> String {
    if sektor == "Spor Salonu" {
        return hizmet_kartlari(
            "Üyelik & Dersler",
            &[
                ("🏋️", "Fitness Salonu", "Alet ve serbest ağırlık alanı."),
                ("🧘", "Grup Dersleri", "Yoga, pilates ve kardiyo."),
                ("🎯", "Kişisel Antrenör", "Hedefe özel program."),
            ],
        );
    }
    hizmet_kartlari(
        "Program & Hizmetler",
        &[
            ("🗓️", "Program", "Güncel etkinlik ve seans saatleri."),
            (
                "🎟️",
                "Rezervasyon",
                "Telefon veya WhatsApp ile yer ayırtma.",
            ),
            ("🎉", "Özel Gün", "Doğum günü, davet ve grup organizasyonu."),
        ],
    )
}

/// OSM `opening_hours` gün kodlarını Türkçeye çevirir; saat ve notlara dokunmaz.
fn gun_kodu_cevir(kelime: &str) -> Option<&'static str> {
    match kelime {
        "Mo" => Some("Pzt"),
        "Tu" => Some("Sal"),
        "We" => Some("Çar"),
        "Th" => Some("Per"),
        "Fr" => Some("Cum"),
        "Sa" => Some("Cmt"),
        "Su" => Some("Paz"),
        "off" => Some("kapalı"),
        _ => None,
    }
}

fn calisma_saatleri_metni(ham: &str) -> String {
    let ham = ham.trim();
    if ham.is_empty() {
        return String::new();
    }
    if ham == "24/7" {
        return "24 saat açık".to_string();
    }

    // Gün kodları yalnızca tam kelime olarak çevrilir; "Sadece" gibi notlar bozulmaz.
    let mut cikti = String::with_capacity(ham.len());
    let mut kelime = String::new();
    for karakter in ham.chars() {
        if karakter.is_ascii_alphabetic() {
            kelime.push(karakter);
        } else {
            if !kelime.is_empty() {
                match gun_kodu_cevir(&kelime) {
                    Some(uzun) => cikti.push_str(uzun),
                    None => cikti.push_str(&kelime),
                }
                kelime.clear();
            }
            cikti.push(karakter);
        }
    }
    if !kelime.is_empty() {
        match gun_kodu_cevir(&kelime) {
            Some(uzun) => cikti.push_str(uzun),
            None => cikti.push_str(&kelime),
        }
    }
    cikti.replace("; ", " · ").replace(';', " · ")
}

fn saat_dakika(metin: &str) -> Option<u32> {
    let (saat, dakika) = metin.trim().split_once(':')?;
    let saat: u32 = saat.parse().ok()?;
    let dakika: u32 = dakika.parse().ok()?;
    if saat > 24 || dakika > 59 {
        return None;
    }
    Some(saat * 60 + dakika)
}

fn gun_indeksi(kod: &str) -> Option<usize> {
    match kod {
        "Mo" => Some(0),
        "Tu" => Some(1),
        "We" => Some(2),
        "Th" => Some(3),
        "Fr" => Some(4),
        "Sa" => Some(5),
        "Su" => Some(6),
        _ => None,
    }
}

fn gun_listesi(metin: &str) -> Option<Vec<usize>> {
    let mut gunler = Vec::new();
    for parca in metin.split(',') {
        if let Some((bas, son)) = parca.split_once('-') {
            let bas = gun_indeksi(bas)?;
            let son = gun_indeksi(son)?;
            let mut gun = bas;
            loop {
                gunler.push(gun);
                if gun == son || gunler.len() > 7 {
                    break;
                }
                gun = (gun + 1) % 7;
            }
        } else {
            gunler.push(gun_indeksi(parca)?);
        }
    }
    if gunler.is_empty() || gunler.len() > 7 {
        None
    } else {
        Some(gunler)
    }
}

/// OSM `opening_hours` değerini Pazartesi=0 olacak şekilde haftalık dakika
/// aralıklarına çözer. Desteklenmeyen (mevsimsel, PH, sunrise gibi) ifadelerde
/// `None` döner; bu durumda demo yalnızca okunur saat metnini gösterir.
fn haftalik_program(ham: &str) -> Option<[Vec<(u32, u32)>; 7]> {
    let ham = ham.trim();
    if ham.is_empty() {
        return None;
    }
    let mut program: [Vec<(u32, u32)>; 7] = Default::default();
    if ham == "24/7" {
        for gun in program.iter_mut() {
            gun.push((0, 1440));
        }
        return Some(program);
    }

    for kural in ham.split(';') {
        let kural = kural.trim();
        if kural.is_empty() {
            continue;
        }
        if kural.contains('"')
            || kural.contains("||")
            || kural.contains("PH")
            || kural.contains("week")
            || kural.contains("sunrise")
            || kural.contains("sunset")
        {
            return None;
        }

        let parcalar: Vec<&str> = kural.split_whitespace().collect();
        let (gunler, saat_metni) = match parcalar.len() {
            1 => (vec![0, 1, 2, 3, 4, 5, 6], parcalar[0]),
            2 => (gun_listesi(parcalar[0])?, parcalar[1]),
            _ => return None,
        };
        if saat_metni == "off" || saat_metni == "closed" {
            continue;
        }

        let mut araliklar = Vec::new();
        for aralik in saat_metni.split(',') {
            let (bas, son) = aralik.split_once('-')?;
            let bas = saat_dakika(bas)?;
            let son = saat_dakika(son)?;
            if bas == son {
                return None;
            }
            araliklar.push((bas, son));
        }

        for gun in gunler {
            for &(bas, son) in &araliklar {
                if bas < son {
                    program[gun].push((bas, son));
                } else {
                    // Gece yarısını aşan aralık iki güne bölünür.
                    program[gun].push((bas, 1440));
                    program[(gun + 1) % 7].push((0, son));
                }
            }
        }
    }

    if program.iter().all(|gun| gun.is_empty()) {
        None
    } else {
        Some(program)
    }
}

fn program_js(program: &[Vec<(u32, u32)>; 7]) -> String {
    let gunler: Vec<String> = program
        .iter()
        .map(|araliklar| {
            let liste: Vec<String> = araliklar
                .iter()
                .map(|(bas, son)| format!("[{bas},{son}]"))
                .collect();
            format!("[{}]", liste.join(","))
        })
        .collect();
    format!("[{}]", gunler.join(","))
}

/// İşletme koordinatlarından OpenStreetMap gömülü haritası üretir.
fn konum_bolumu_html(isletme_adi: &str, enlem: f64, boylam: f64) -> String {
    let ad = html_kacis(isletme_adi);
    let boylam_pay = 0.004;
    let enlem_pay = 0.003;
    let bbox = format!(
        "{:.4},{:.4},{:.4},{:.4}",
        boylam - boylam_pay,
        enlem - enlem_pay,
        boylam + boylam_pay,
        enlem + enlem_pay
    );
    let embed = format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={}&layer=mapnik&marker={enlem:.5},{boylam:.5}",
        url_encode(&bbox)
    );
    let link = format!(
        "https://www.openstreetmap.org/?mlat={enlem:.5}&mlon={boylam:.5}#map=18/{enlem:.5}/{boylam:.5}"
    );
    format!(
        r##"<section class="mt-14">
<h2 class="text-2xl font-bold mb-6">Konum</h2>
<div class="rounded-xl overflow-hidden border border-zinc-800">
<iframe title="{ad} konumu" width="100%" height="320" style="border:0" loading="lazy" src="{embed}"></iframe>
</div>
<a class="inline-block mt-3 text-amber-400 text-sm hover:underline" href="{link}" target="_blank" rel="noopener">Haritada aç</a>
</section>
"##
    )
}

fn demo_html(isletme: &BusinessLead, gonderen: &str) -> String {
    let ad = html_kacis(&isletme.name);
    let adres = html_kacis(&isletme.address);
    let sektor = html_kacis(&isletme.category);
    let bolum = demo_bolumu(&isletme.category);
    let kaynak = html_kacis(&isletme.kaynak);
    // Karne, demo ile aynı klasöre aynı ad köküyle yazılır: göreli bağlantı yeter.
    let karne_linki = format!(
        "<p class=\"mt-2\"><a class=\"text-amber-400 hover:underline\" href=\"{}\">Dijital görünürlük karnesi</a></p>",
        karne_dosya_adi(isletme)
    );
    let konum = match isletme.konum {
        Some((enlem, boylam)) => konum_bolumu_html(&isletme.name, enlem, boylam),
        None => String::new(),
    };
    let saatler = calisma_saatleri_metni(&isletme.calisma_saatleri);
    let (saat_satiri, durum_script) = if saatler.is_empty() {
        (String::new(), String::new())
    } else {
        let okunur = html_kacis(&saatler);
        match haftalik_program(&isletme.calisma_saatleri) {
            Some(program) => (
                format!(
                    "<p class=\"mt-1\">🕒 {okunur}</p>\n<p id=\"acik-durum\" class=\"mt-1\"></p>"
                ),
                format!(
                    r##"<script>
const program = {program};
const gunAdlari = ["Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi", "Pazar"];
function ikiHane(sayi) {{ return String(sayi).padStart(2, "0"); }}
function saatMetni(dakika) {{ return ikiHane(Math.floor(dakika / 60) % 24) + ":" + ikiHane(dakika % 60); }}
function kalanMetni(dakika) {{
  const gun = Math.floor(dakika / 1440);
  const saat = Math.floor((dakika % 1440) / 60);
  const dk = dakika % 60;
  const parcalar = [];
  if (gun > 0) parcalar.push(gun + " gün");
  if (saat > 0) parcalar.push(saat + " sa");
  if (dk > 0) parcalar.push(dk + " dk");
  return parcalar.length ? parcalar.join(" ") : "0 dk";
}}
function durumHesapla() {{
  const simdi = new Date();
  const gun = (simdi.getDay() + 6) % 7;
  const dakika = simdi.getHours() * 60 + simdi.getMinutes();
  for (const [bas, son] of (program[gun] || [])) {{
    if (dakika >= bas && dakika < son) {{
      if (bas === 0 && son >= 1440) return "🟢 Şu an açık · 24 saat";
      if (son >= 1440) return "🟢 Şu an açık · gece yarısına kadar";
      const kalan = son - dakika;
      const ayrinti = "kapanış " + saatMetni(son) + " (" + kalanMetni(kalan) + " kaldı)";
      return kalan <= 15 ? "🟠 Kapanmak üzere · " + ayrinti : "🟢 Şu an açık · " + ayrinti;
    }}
  }}
  for (let i = 0; i < 7; i++) {{
    const hedef = (gun + i) % 7;
    for (const [bas] of (program[hedef] || [])) {{
      if (i === 0 && bas <= dakika) continue;
      const gunEtiketi = i === 0 ? "bugün" : (i === 1 ? "yarın" : gunAdlari[hedef]);
      const acilisa = i * 1440 + bas - dakika;
      return "🔴 Şu an kapalı · açılış " + gunEtiketi + " " + saatMetni(bas) + " · " + kalanMetni(acilisa) + " kaldı";
    }}
  }}
  return "🔴 Şu an kapalı";
}}
function durumGuncelle() {{
  const durum = document.getElementById("acik-durum");
  if (!durum) return;
  const mesaj = durumHesapla();
  const renk = mesaj.startsWith("🟢")
    ? "text-emerald-400"
    : (mesaj.startsWith("🟠") ? "text-amber-400" : "text-rose-400");
  durum.textContent = mesaj;
  durum.className = "mt-1 text-sm font-semibold " + renk;
}}
durumGuncelle();
setInterval(durumGuncelle, 60000);
</script>
"##,
                    program = program_js(&program)
                ),
            ),
            None => (format!("<p class=\"mt-1\">🕒 {okunur}</p>"), String::new()),
        }
    };
    let anahtar = dosya_slug(&isletme.name);
    let hazirlayan = if gonderen.trim().is_empty() {
        "Nexus Enterprise".to_string()
    } else {
        html_kacis(gonderen)
    };
    // Open Graph: link WhatsApp/SMS önizlemesinde başlıklı ve açıklamalı
    // görünsün. Etiketler, tanımlayıcı alanın hemen altında durur.
    let ozet_metni = if isletme.address.is_empty() {
        format!("{sektor} · {ad} için hazırlanmış mobil uyumlu site taslağı")
    } else {
        format!("{sektor} · {adres} · {ad} için hazırlanmış mobil uyumlu site taslağı")
    };
    let paylasim = format!(
        concat!(
            "<meta name=\"description\" content=\"{ozet}\">\n",
            "<meta property=\"og:type\" content=\"website\">\n",
            "<meta property=\"og:site_name\" content=\"{ad}\">\n",
            "<meta property=\"og:title\" content=\"{ad} — {sektor}\">\n",
            "<meta property=\"og:description\" content=\"{ozet}\">\n",
            "<meta property=\"og:locale\" content=\"tr_TR\">\n",
            "<meta name=\"twitter:card\" content=\"summary\">\n",
            "<meta name=\"robots\" content=\"noindex\">"
        ),
        ad = ad,
        sektor = sektor,
        ozet = ozet_metni
    );
    let tel_blogu = match whatsapp_numarasi(&isletme.phone) {
        Some(tel) => format!(
            "<a href=\"tel:+{tel}\" class=\"inline-block bg-amber-500 text-zinc-950 font-bold px-6 py-3 rounded-full\">📞 Hemen Ara</a>"
        ),
        None => String::new(),
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{ad} — {sektor}</title>
{paylasim}
<script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-zinc-950 text-zinc-100 font-sans">
<div class="bg-amber-500 text-black text-xs font-bold text-center py-2 px-4">12 SAAT GEÇERLİ TASLAK DEMO — {ad} için hazırlandı</div>
<main class="max-w-4xl mx-auto px-4 py-16">
<p class="text-amber-400 text-xs font-bold uppercase tracking-widest mb-4">{sektor}</p>
<h1 class="text-4xl md:text-5xl font-black mb-4">{ad}</h1>
<p class="text-zinc-400 mb-2">📍 {adres}</p>
<p class="text-zinc-400 max-w-xl mt-6">İşletmeniz için hazırlanmış modern ve mobil uyumlu web sitesi taslağı. Menü, galeri, harita ve iletişim formu bölümleri eklenebilir; kendi alan adınızda yayına alınabilir.</p>
<div class="mt-8 flex flex-wrap gap-3">
{tel_blogu}
<a href="#iletisim" class="inline-block border border-zinc-700 text-zinc-200 px-6 py-3 rounded-full">İletişim</a>
</div>
{bolum}
{konum}
<section id="iletisim" class="mt-16 pt-8 border-t border-zinc-800 text-sm text-zinc-400">
<p>Bu taslak {hazirlayan} tarafından {ad} için hazırlandı.</p>
{karne_linki}
{saat_satiri}
<p class="mt-1">Demo süresi: 12 saat · Kaynak: {kaynak}</p>
</section>
</main>
{durum_script}
<script>
const anahtar = "demo_{anahtar}";
const baslangic = Number(localStorage.getItem(anahtar) || Date.now());
localStorage.setItem(anahtar, baslangic);
if (Date.now() - baslangic > 12 * 60 * 60 * 1000) {{
  document.body.innerHTML = '<div style="min-height:100vh;display:flex;align-items:center;justify-content:center;background:#18181b;color:#fff;text-align:center;padding:24px;font-family:sans-serif"><h1 style="color:#ef4444">Demo süresi doldu</h1></div>';
}}
</script>
</body>
</html>
"##
    )
}

fn demo_uret(isletme: &BusinessLead, gonderen: &str) -> Result<String, String> {
    demo_yaz(DEMO_KLASORU, isletme, gonderen)
}

fn demo_yaz(klasor: &str, isletme: &BusinessLead, gonderen: &str) -> Result<String, String> {
    fs::create_dir_all(klasor)
        .map_err(|hata| format!("{klasor} klasörü oluşturulamadı: {hata}"))?;
    let dosya_adi = demo_dosya_adi(isletme);
    let dosya = format!("{klasor}/{dosya_adi}");
    fs::write(&dosya, demo_html(isletme, gonderen))
        .map_err(|hata| format!("{dosya} yazılamadı: {hata}"))?;
    // Karne demo ile birlikte üretilir: aynı klasörde olduğu için demoda ona
    // göreli bağlantı var, satış görüşmesinde ikisi birlikte açılır.
    karne_yaz(klasor, isletme, Some(&dosya_adi))?;
    Ok(dosya)
}

/// Kayıt kimliği: OSM node/way numarası veya Google place id'sinin son parçası.
/// Demo ve karne dosya adlarının sonuna eklenir, böylece aynı adlı iki işletme
/// birbirinin dosyasını ezmez.
fn kayit_kimligi(isletme: &BusinessLead) -> String {
    isletme
        .kaynak
        .rsplit('/')
        .next()
        .unwrap_or("0")
        .chars()
        .map(|karakter| {
            if karakter.is_ascii_alphanumeric() || karakter == '-' {
                karakter
            } else {
                '_'
            }
        })
        .collect()
}

/// Demo dosyasının adı (klasörsüz). Karne, demoya göreli bağlantı verebilmek
/// için aynı klasöre yazılır.
fn demo_dosya_adi(isletme: &BusinessLead) -> String {
    format!(
        "{}-{}.html",
        dosya_slug(&isletme.name),
        kayit_kimligi(isletme)
    )
}

/// Dijital görünürlük karnesinin dosya adı: `karne-<isletme>-<kimlik>.html`.
fn karne_dosya_adi(isletme: &BusinessLead) -> String {
    format!(
        "karne-{}-{}.html",
        dosya_slug(&isletme.name),
        kayit_kimligi(isletme)
    )
}

/// Karnedeki tek kontrolün durumu (rozet rengi ve simgesi buradan gelir).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KarneDurumu {
    /// Eksik yok: işletme bu kontrolü geçiyor.
    Iyi,
    /// Küçük eksik: kapatılabilir, satışı engellemez.
    Orta,
    /// Satışı doğrudan gerekçelendiren eksik.
    Eksik,
}

/// Dijital görünürlük karnesindeki tek satır: ne bulundu, ne kadar puan
/// kaybettirdi ve müşteriye ne önerilecek.
struct KarneSatiri {
    baslik: &'static str,
    deger: String,
    durum: KarneDurumu,
    /// Bu eksikliğin 100 üzerinden puan kaybı. 0 ise kontrol geçildi.
    kayip: u8,
    oneri: &'static str,
}

fn karne_satiri(
    baslik: &'static str,
    deger: String,
    kayip: u8,
    oneri: &'static str,
) -> KarneSatiri {
    KarneSatiri {
        durum: match kayip {
            0 => KarneDurumu::Iyi,
            1..=6 => KarneDurumu::Orta,
            _ => KarneDurumu::Eksik,
        },
        baslik,
        deger,
        kayip,
        oneri,
    }
}

/// Yıldız puanını Türkçe ondalık ayracıyla yazar: 4.2 → "4,2".
fn puan_metni(puan: f32) -> String {
    format!("{puan:.1}").replace('.', ",")
}

/// İşletmenin dijital görünürlük karnesini ölçer: 100 üzerinden puan ve
/// kontrol satırları. Yalnızca taramada zaten toplanmış veriler kullanılır;
/// hiçbir ağ isteği yapılmaz, yani karne Google kotası harcamaz.
///
/// Ağırlıklar (toplam 90 puan kaybı mümkün, taban puan 10):
///   web sitesi yok                  -40   (sosyal medya varsa -30,
///                                          ücretsiz site aracı varsa -20)
///   çalışma saatleri kayıtlı değil  -20
///   telefon kayıtlı değil           -15
///   Google yorum/puan yok           -15   (10+ yorum -5, 50+ yorum 0,
///                                          4'ün altındaki puan ayrıca ceza)
fn karne_olc(isletme: &BusinessLead) -> (u8, Vec<KarneSatiri>) {
    let link = if isletme.site_linki.trim().is_empty() {
        String::new()
    } else {
        format!(" · {}", isletme.site_linki.trim())
    };
    let (site_degeri, site_kayip, site_oneri) = match isletme.site {
        SiteDurumu::Yok => (
            "Web sitesi yok · sosyal medya hesabı da bulunamadı".to_string(),
            40,
            "Kendi alan adınızda mobil uyumlu bir site açın; Google profilinize ekleyin.",
        ),
        SiteDurumu::Sosyal => (
            format!("Web sitesi yok, yalnızca sosyal medya hesabı var{link}"),
            30,
            "Sosyal medya profiline ek olarak kendi alan adınızda bir site açın.",
        ),
        SiteDurumu::Ucretsiz => (
            format!("Web sitesi yok, ücretsiz site aracıyla kurulmuş sayfa var{link}"),
            20,
            "Ücretsiz alt alan adındaki sayfayı kendi alan adınıza taşıyın.",
        ),
    };

    let saat_metni = calisma_saatleri_metni(&isletme.calisma_saatleri);
    let (saat_degeri, saat_kayip) = if saat_metni.is_empty() {
        ("Çalışma saatleri kayıtlı değil".to_string(), 20)
    } else {
        (format!("Kayıtlı program: {saat_metni}"), 0)
    };

    let (telefon_degeri, telefon_kayip) = if isletme.phone.trim().is_empty() {
        ("Telefon numarası kayıtlı değil".to_string(), 15)
    } else if whatsapp_numarasi(&isletme.phone).is_some() {
        (
            format!("{} · WhatsApp ile ulaşılabilir", isletme.phone.trim()),
            0,
        )
    } else {
        (
            format!("{} · WhatsApp üzerinden ulaşılamıyor", isletme.phone.trim()),
            5,
        )
    };

    let mut yorum_parcalari: Vec<String> = Vec::new();
    let mut yorum_kayip: u8 = 15;
    if let Some(adet) = isletme.inceleme_sayisi {
        yorum_parcalari.push(format!("{adet} yorum"));
        yorum_kayip = match adet {
            50.. => 0,
            10..=49 => 5,
            1..=9 => 10,
            _ => 14,
        };
    }
    if let Some(puan) = isletme.yildiz_puani {
        yorum_parcalari.push(format!("{} ★", puan_metni(puan)));
        // Düşük puan, yorum sayısından bağımsız bir güven sorunudur.
        yorum_kayip = match puan {
            p if p < 3.5 => yorum_kayip.saturating_add(4),
            p if p < 4.0 => yorum_kayip.saturating_add(2),
            _ => yorum_kayip,
        };
    }
    let (yorum_degeri, yorum_kayip) = if yorum_parcalari.is_empty() {
        (
            "Google'da yorum ve puan verisi yok".to_string(),
            yorum_kayip,
        )
    } else {
        (yorum_parcalari.join(" · "), yorum_kayip)
    };

    let satirlar = vec![
        karne_satiri("Web sitesi", site_degeri, site_kayip, site_oneri),
        karne_satiri(
            "Çalışma saatleri",
            saat_degeri,
            saat_kayip,
            "Google işletme profilinize haftalık saatleri girin; \"şu an açık\" bilgisi görünür.",
        ),
        karne_satiri(
            "Telefon",
            telefon_degeri,
            telefon_kayip,
            "Profilinize tıklanabilir bir telefon numarası ekleyin.",
        ),
        karne_satiri(
            "Google yorumları",
            yorum_degeri,
            yorum_kayip,
            "Müşterilerinizden Google yorumu isteyin; 10 ve üzeri yorum güven oluşturur.",
        ),
    ];

    let kayip: u16 = satirlar.iter().map(|satir| satir.kayip as u16).sum();
    let puan = (100u16.saturating_sub(kayip)).clamp(10, 100) as u8;
    (puan, satirlar)
}

/// Puanın sözel karşılığı ve Tailwind renk adı.
fn karne_sinifi(puan: u8) -> (&'static str, &'static str) {
    match puan {
        80.. => ("İyi", "emerald"),
        60..=79 => ("Orta", "amber"),
        40..=59 => ("Zayıf", "orange"),
        _ => ("Kritik", "rose"),
    }
}

/// Tek sayfalık dijital görünürlük karnesi. Satış görüşmesinde müşteriye
/// gösterilecek belge: eksikleri, neden önemli olduğunu ve önerilen adımları
/// tek ekranda toplar. `demo_dosyasi` verilirse hazırlanan taslağa bağlantı
/// eklenir (aynı klasörde olduğu için göreli adres yeterli).
fn karne_html(isletme: &BusinessLead, demo_dosyasi: Option<&str>) -> String {
    let (puan, satirlar) = karne_olc(isletme);
    let (sinif, renk) = karne_sinifi(puan);
    let ad = html_kacis(&isletme.name);
    let sektor = html_kacis(&isletme.category);
    let adres = html_kacis(&isletme.address);
    let kaynak = html_kacis(&isletme.kaynak);
    let tarih = bugun_anahtari();

    let eksikler: Vec<&KarneSatiri> = satirlar.iter().filter(|satir| satir.kayip > 0).collect();
    let eksik_sayisi = eksikler.len();
    let tamam_sayisi = satirlar.len() - eksik_sayisi;
    let ozet = if eksikler.is_empty() {
        "Bu karnedeki dört kontrolün hepsi geçildi; acil bir eksik görünmüyor.".to_string()
    } else {
        format!(
            "{} eksik tespit edildi: {}. Müşteri işletmenizi Google'da ararken bu bilgileri göremediğinde doğrudan rakip işletmeye geçiyor.",
            eksikler.len(),
            eksikler
                .iter()
                .map(|satir| satir.baslik)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let satir_html: String = satirlar
        .iter()
        .map(|satir| {
            let (rozet, rozet_sinif) = match satir.durum {
                KarneDurumu::Iyi => ("✓", "border-emerald-500/40 bg-emerald-500/15 text-emerald-300"),
                KarneDurumu::Orta => ("!", "border-amber-500/40 bg-amber-500/15 text-amber-300"),
                KarneDurumu::Eksik => ("✕", "border-rose-500/40 bg-rose-500/15 text-rose-300"),
            };
            let etiket = match satir.durum {
                KarneDurumu::Iyi => "tamam",
                KarneDurumu::Orta => "kısmi",
                KarneDurumu::Eksik => "eksik",
            };
            format!(
                r##"<div class="flex items-start gap-3 p-4">
<span class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-xs font-bold {rozet_sinif}">{rozet}</span>
<div class="min-w-0">
<p class="text-sm font-semibold">{baslik} <span class="ml-1 text-[10px] font-bold uppercase tracking-wide text-zinc-500">{etiket}</span></p>
<p class="mt-0.5 break-words text-sm text-zinc-400">{deger}</p>
</div>
</div>
"##,
                baslik = html_kacis(satir.baslik),
                deger = html_kacis(&satir.deger),
            )
        })
        .collect();

    let adim_html: String = eksikler
        .iter()
        .enumerate()
        .map(|(sira, satir)| {
            format!(
                r##"<li class="flex gap-3"><span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-zinc-800 text-xs font-bold text-amber-300">{}</span><span>{}</span></li>"##,
                sira + 1,
                html_kacis(satir.oneri)
            )
        })
        .collect();

    let adim_bolumu = if eksikler.is_empty() {
        String::new()
    } else {
        format!(
            r##"<h2 class="mt-10 text-lg font-bold">Önerilen adımlar</h2>
<ol class="mt-3 space-y-2 text-sm text-zinc-300">{adim_html}</ol>
"##
        )
    };

    let demo_blok = match demo_dosyasi {
        Some(dosya) => format!(
            r##"<section class="mt-10 rounded-xl border border-amber-500/30 bg-amber-500/5 p-5">
<h2 class="text-base font-bold text-amber-300">Hazırlanan site taslağı</h2>
<p class="mt-1 text-sm text-zinc-400">Bu karnedeki eksikleri kapatan örnek sayfa sizin için hazırlandı.</p>
<a class="mt-3 inline-block rounded-full bg-amber-500 px-5 py-2 text-sm font-bold text-black" href="{href}">Taslağı aç</a>
</section>
"##,
            href = html_kacis(dosya)
        ),
        None => String::new(),
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{ad} — Dijital Görünürlük Karnesi</title>
<script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-zinc-950 font-sans text-zinc-100">
<main class="mx-auto max-w-3xl px-5 py-12">
<p class="text-xs font-bold uppercase tracking-widest text-amber-400">Dijital Görünürlük Karnesi</p>
<h1 class="mt-2 text-3xl font-black md:text-4xl">{ad}</h1>
<p class="mt-1 text-sm text-zinc-400">{sektor} · {adres}</p>
<p class="mt-1 text-xs text-zinc-500">Rapor tarihi: {tarih} · Kaynak: <a class="hover:underline" href="{kaynak}" target="_blank" rel="noopener">işletme kaydı</a></p>

<section class="mt-8 grid grid-cols-1 gap-4 sm:grid-cols-3">
<div class="rounded-xl border border-{renk}-500/40 bg-{renk}-500/10 p-5">
<p class="text-xs text-zinc-400">Dijital görünürlük puanı</p>
<p class="mt-1 text-5xl font-black text-{renk}-400">{puan}</p>
<p class="text-xs text-zinc-400">100 üzerinden</p>
<p class="mt-3 text-sm font-bold text-{renk}-300">{sinif}</p>
</div>
<div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5 sm:col-span-2">
<p class="text-xs uppercase tracking-widest text-zinc-500">Özet</p>
<p class="mt-2 text-sm text-zinc-300">{ozet}</p>
<p class="mt-3 text-xs text-zinc-500">{eksik_sayisi} eksik · {tamam_sayisi} tamam</p>
</div>
</section>

<h2 class="mt-10 text-lg font-bold">Tespitler</h2>
<div class="mt-3 divide-y divide-zinc-800 rounded-xl border border-zinc-800 bg-zinc-900/40">
{satir_html}</div>
{adim_bolumu}
{demo_blok}

<section class="mt-10 border-t border-zinc-800 pt-5 text-xs leading-relaxed text-zinc-500">
<p>Bu karne, işletmenizin herkese açık kayıt bilgilerinden (Google / OpenStreetMap) otomatik olarak üretildi; hiçbir bilgi tahmin edilmedi. Eksik görünen bir bilgi varsa kaydınızı güncellemeniz karnedeki puanı da yükseltir.</p>
<p class="mt-2">Sektör: {sektor} · Kayıt: {kaynak}</p>
</section>
</main>
</body>
</html>
"##
    )
}

/// Karneyi yazar. Demo ile aynı klasöre gider; `demo_dosyasi` verilirse
/// karneden taslağa göreli bağlantı konur.
fn karne_yaz(
    klasor: &str,
    isletme: &BusinessLead,
    demo_dosyasi: Option<&str>,
) -> Result<String, String> {
    fs::create_dir_all(klasor)
        .map_err(|hata| format!("{klasor} klasörü oluşturulamadı: {hata}"))?;
    let dosya = format!("{klasor}/{}", karne_dosya_adi(isletme));
    fs::write(&dosya, karne_html(isletme, demo_dosyasi))
        .map_err(|hata| format!("{dosya} yazılamadı: {hata}"))?;
    Ok(dosya)
}

/// Karneyi üretir (gerekirse demoyu da üretir) ve tarayıcıda açar.
fn karne_ac(lead: &mut BusinessLead, gonderen: &str, logs: &mut Vec<String>) {
    if lead.demo_yolu.is_none() {
        match demo_uret(lead, gonderen) {
            Ok(yol) => lead.demo_yolu = Some(yol),
            Err(hata) => {
                logs.push(format!("[!] {hata}"));
                return;
            }
        }
    }
    let dosya = format!("{DEMO_KLASORU}/{}", karne_dosya_adi(lead));
    if !std::path::Path::new(&dosya).exists() {
        if let Err(hata) = karne_yaz(DEMO_KLASORU, lead, Some(&demo_dosya_adi(lead))) {
            logs.push(format!("[!] {hata}"));
            return;
        }
    }
    match tarayicida_ac(&dosya) {
        Ok(()) => logs.push(format!(
            "[✓] {} için dijital görünürlük karnesi açıldı: {dosya}",
            lead.name
        )),
        Err(hata) => logs.push(format!("[!] Karne açılamadı: {hata}")),
    }
}

/// Yayın çalışma kopyasının bulunduğu klasör (asıl çalışma ağacının dışında,
/// `gh-pages` dalına bağlı ayrı bir git worktree).
const YAYIN_KLASORU: &str = ".yayin";
/// Demo dosyalarının yayınlandığı dal. GitHub Pages bu dalı kök olarak yayınlar.
const YAYIN_DALI: &str = "gh-pages";

/// Dış komutu sessiz çalıştırır (arayüzden çağrıldığı için konsol penceresi
/// açılmaz) ve çıktısını döndürür; başarısızlıkta `stderr` metnini taşır.
fn komut_calistir(dizin: &str, program: &str, args: &[&str]) -> Result<String, String> {
    let mut emir = Command::new(program);
    emir.args(args).current_dir(dizin);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: git çağrıları için konsol penceresi açılmasın.
        emir.creation_flags(0x0800_0000);
    }
    let cikti = emir
        .output()
        .map_err(|hata| format!("{program} çalıştırılamadı: {hata}"))?;
    if !cikti.status.success() {
        let hata = String::from_utf8_lossy(&cikti.stderr).trim().to_string();
        return Err(format!("{program} {}: {hata}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&cikti.stdout).to_string())
}

/// Uzak depo adresinden GitHub sahibini ve depo adını çıkarır.
/// `https://github.com/owner/repo.git`, `git@github.com:owner/repo.git` ve
/// sonunda `.git` olmayan biçimler desteklenir.
fn github_depo(uzak: &str) -> Option<(String, String)> {
    let temiz = uzak.trim();
    let kalan = temiz.split("github.com").nth(1)?;
    let kalan = kalan.trim_start_matches(['/', ':']);
    let parcalar: Vec<&str> = kalan.split('/').filter(|parca| !parca.is_empty()).collect();
    if parcalar.len() < 2 {
        return None;
    }
    let depo = parcalar[1].trim_end_matches(".git");
    if depo.is_empty() {
        return None;
    }
    Some((parcalar[0].to_string(), depo.to_string()))
}

/// Bir demo dosyasının paylaşılabilir GitHub Pages adresi:
/// `https://<sahip>.github.io/<depo>/demos/<dosya>`.
fn yayin_adresi(sahip: &str, depo: &str, dosya: &str) -> String {
    format!(
        "https://{}.github.io/{}/demos/{}",
        sahip.to_lowercase(),
        depo,
        dosya
    )
}

/// Yayın kök sayfası: yayınlanmış tüm demoları listeleyen sade bir dizin.
fn yayin_dizini(dosyalar: &[String]) -> String {
    let mut satirlar = String::new();
    let mut sirali: Vec<&String> = dosyalar.iter().collect();
    sirali.sort();
    for dosya in sirali {
        let etiket = dosya
            .trim_end_matches(".html")
            .trim_start_matches("karne-")
            .replace('-', " ");
        satirlar.push_str(&format!(
            "<li><a href=\"demos/{}\">{}</a></li>\n",
            html_kacis(dosya),
            html_kacis(&etiket)
        ));
    }
    format!(
        r##"<!DOCTYPE html>
<html lang="tr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Demo yayını</title>
<style>
body{{background:#0b0e13;color:#eaeef5;font-family:system-ui,sans-serif;margin:0;padding:48px 20px}}
main{{max-width:720px;margin:0 auto}}
h1{{font-size:22px;margin:0 0 6px}}
p{{color:#8b95a7;font-size:13px;margin:0 0 24px}}
ul{{list-style:none;padding:0;margin:0;display:grid;gap:8px}}
a{{display:block;padding:12px 14px;border:1px solid #232a36;border-radius:10px;color:#eaeef5;text-decoration:none;background:#12161d}}
a:hover{{border-color:#3a4557}}
</style>
</head>
<body>
<main>
<h1>Hazırlanan demo sayfaları</h1>
<p>Bu bağlantılar taslak amaçlıdır; işletmelerin onayı olmadan yayına alınmaz.</p>
<ul>
{satirlar}</ul>
</main>
</body>
</html>
"##
    )
}

/// `gh-pages` dalına bağlı yayın çalışma kopyasını hazırlar.
fn yayin_hazirla(kok: &str) -> Result<(), String> {
    let klasor = format!("{kok}/{YAYIN_KLASORU}");
    if std::path::Path::new(&klasor).join(".git").exists() {
        return Ok(());
    }
    if std::path::Path::new(&klasor).exists() {
        return Err(format!(
            "{YAYIN_KLASORU} klasörü var ama bir git worktree değil; silip yeniden deneyin."
        ));
    }
    let dal_var = komut_calistir(
        kok,
        "git",
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{YAYIN_DALI}"),
        ],
    )
    .is_ok();
    if dal_var {
        komut_calistir(kok, "git", &["worktree", "add", YAYIN_KLASORU, YAYIN_DALI])
            .map(|_| ())
            .map_err(|hata| format!("{hata} (gerekirse: git worktree prune)"))?;
    } else {
        komut_calistir(kok, "git", &["worktree", "add", "--detach", YAYIN_KLASORU])
            .map_err(|hata| format!("{hata} (gerekirse: git worktree prune)"))?;
        komut_calistir(&klasor, "git", &["checkout", "--orphan", YAYIN_DALI])?;
        // Orphan dalda dizin boşaltılır; hata (boş dizin) yok sayılır.
        let _ = komut_calistir(&klasor, "git", &["rm", "-rf", "."]);
    }
    Ok(())
}

/// Demoyu (ve varsa karneyi) GitHub Pages dalına yayınlar ve paylaşılabilir
/// adresi döndürür. Yalnızca değişen dosyalar commit'lenir; aynı demo ikinci
/// kez yayınlandığında yeni commit oluşmaz.
fn yayina_gonder(kok: &str, dosyalar: &[String], logs: &mut Vec<String>) -> Result<String, String> {
    let klasor = format!("{kok}/{YAYIN_KLASORU}");
    let hedef_klasor = format!("{klasor}/demos");
    fs::create_dir_all(&hedef_klasor)
        .map_err(|hata| format!("{hedef_klasor} oluşturulamadı: {hata}"))?;

    let mut yayinlanan: Vec<String> = Vec::new();
    for kaynak in dosyalar {
        let ad = std::path::Path::new(kaynak)
            .file_name()
            .map(|ad| ad.to_string_lossy().to_string())
            .unwrap_or_default();
        if ad.is_empty() {
            continue;
        }
        // Karne demoya göreli bağlantı verdiği için ikisi de aynı klasöre gider.
        fs::copy(kaynak, format!("{hedef_klasor}/{ad}"))
            .map_err(|hata| format!("{ad} kopyalanamadı: {hata}"))?;
        yayinlanan.push(ad);
    }
    if yayinlanan.is_empty() {
        return Err("Yayınlanacak dosya yok.".to_string());
    }

    // Kök dizin, yayınlanmış tüm demoları listeleyen sayfa olur.
    let mut mevcut: Vec<String> = fs::read_dir(&hedef_klasor)
        .map(|kayitlar| {
            kayitlar
                .filter_map(|kayit| kayit.ok())
                .filter(|kayit| {
                    kayit
                        .path()
                        .extension()
                        .is_some_and(|uzanti| uzanti == "html")
                })
                .map(|kayit| kayit.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    mevcut.sort();
    fs::write(format!("{klasor}/index.html"), yayin_dizini(&mevcut))
        .map_err(|hata| format!("index.html yazılamadı: {hata}"))?;

    if komut_calistir(&klasor, "git", &["add", "-A"]).is_err() {
        // Boş depoda ilk ekleme; yine de devam edilir.
    }
    let degisiklik = komut_calistir(&klasor, "git", &["status", "--porcelain"])?;
    if !degisiklik.trim().is_empty() {
        let ad = komut_calistir(kok, "git", &["config", "user.name"])
            .unwrap_or_default()
            .trim()
            .to_string();
        let eposta = komut_calistir(kok, "git", &["config", "user.email"])
            .unwrap_or_default()
            .trim()
            .to_string();
        let ad = if ad.is_empty() {
            "Demo Yayını".to_string()
        } else {
            ad
        };
        let eposta = if eposta.is_empty() {
            "demo@localhost".to_string()
        } else {
            eposta
        };
        let mesaj = format!("Demo yayını: {}", yayinlanan.join(", "));
        komut_calistir(
            &klasor,
            "git",
            &[
                "-c",
                &format!("user.name={ad}"),
                "-c",
                &format!("user.email={eposta}"),
                "commit",
                "-m",
                &mesaj,
            ],
        )?;
    } else {
        logs.push("[i] Yayında değişiklik yok; mevcut link kullanılabilir.".to_string());
    }

    let uzak = komut_calistir(kok, "git", &["remote", "get-url", "origin"])?;
    let (sahip, depo) = github_depo(&uzak)
        .ok_or_else(|| format!("origin adresi GitHub deposu değil: {}", uzak.trim()))?;
    komut_calistir(&klasor, "git", &["push", "origin", YAYIN_DALI])?;
    Ok(yayin_adresi(&sahip, &depo, &yayinlanan[0]))
}

/// Demoyu (yoksa önce üretip) GitHub Pages'e yayınlar ve adresi döndürür.
/// Yayın başarısız olursa `None` döner; çağıranlar akışı durdurmadan devam eder.
/// Karneyi yayına hazırlar: dosya yoksa üretir ve yolunu döndürür. Üretilemezse
/// `None` döner; bu durumda mesaja karne linki eklenmez (kırık link verilmez).
/// Yayındaki demo sayfasının kösesine karne bağlantısı ekler. Böylece mesajda
/// tek link kalır ama karne demo üzerinden tek tıkla açılır.
fn karne_rozeti_ekle(demo_yolu: &str, karne_adi: &str) -> Result<(), String> {
    let html =
        fs::read_to_string(demo_yolu).map_err(|hata| format!("{demo_yolu} okunamadı: {hata}"))?;
    // Aynı demo birden çok kez yayınlanırsa rozet çoğalmasın.
    if html.contains(karne_adi) {
        return Ok(());
    }
    let rozet = format!(
        "<a href=\"{karne_adi}\" style=\"position:fixed;right:16px;bottom:16px;z-index:9999;display:inline-block;background:#0f172a;color:#f8fafc;padding:10px 16px;border-radius:999px;font:600 13px/1.2 system-ui,sans-serif;text-decoration:none;box-shadow:0 6px 20px rgba(15,23,42,.35)\">Eksikler ve puan → dijital karne</a>"
    );
    let yeni = match html.rfind("</body>") {
        Some(yer) => format!("{}{rozet}{}", &html[..yer], &html[yer..]),
        None => format!("{html}{rozet}"),
    };
    fs::write(demo_yolu, yeni).map_err(|hata| format!("{demo_yolu} yazılamadı: {hata}"))
}

fn karne_hazirla(klasor: &str, lead: &BusinessLead, logs: &mut Vec<String>) -> Option<String> {
    let dosya = format!("{klasor}/{}", karne_dosya_adi(lead));
    if !std::path::Path::new(&dosya).exists() {
        if let Err(hata) = karne_yaz(klasor, lead, Some(&demo_dosya_adi(lead))) {
            logs.push(format!("[!] Karne üretilemedi: {hata}"));
        }
    }
    std::path::Path::new(&dosya).exists().then_some(dosya)
}

/// Yayındaki bir dosyanın adresini, demoyla aynı klasördeki başka bir dosya
/// için üretir: `…/demos/x.html` + `karne-x.html` → `…/demos/karne-x.html`.
fn yayin_alt_adresi(demo_adresi: &str, dosya: &str) -> String {
    match demo_adresi.rsplit_once('/') {
        Some((taban, _)) => format!("{taban}/{dosya}"),
        None => demo_adresi.to_string(),
    }
}

fn yayina_gonder_ve_link_al(
    lead: &mut BusinessLead,
    gonderen: &str,
    logs: &mut Vec<String>,
) -> Option<String> {
    if lead.demo_yolu.is_none() {
        match demo_uret(lead, gonderen) {
            Ok(yol) => lead.demo_yolu = Some(yol),
            Err(hata) => {
                logs.push(format!("[!] {hata}"));
                return None;
            }
        }
    }
    let demo_yolu = lead.demo_yolu.clone()?;
    let kok = std::env::current_dir()
        .map(|dizin| dizin.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    if let Err(hata) = yayin_hazirla(&kok) {
        logs.push(format!("[!] Yayın hazırlanamadı: {hata}"));
        return None;
    }

    // Karne demo ile birlikte yayınlanır: dosya yoksa (eski demo, elle açılan
    // dosya) yayından önce burada üretilir ki mesajdaki link boşa çıkmasın.
    let karne_dosyasi = karne_dosya_adi(lead);
    let karne_yolu = karne_hazirla(DEMO_KLASORU, lead, logs);

    // Karne yayına gidecekse demo sayfasına bağlantısı iliştirilir ki mesajda
    // ikinci bir adres yer almasın.
    if karne_yolu.is_some() {
        if let Err(hata) = karne_rozeti_ekle(&demo_yolu, &karne_dosyasi) {
            logs.push(format!("[!] Karne rozeti eklenemedi: {hata}"));
        }
    }

    let mut dosyalar = vec![demo_yolu];
    if let Some(yol) = karne_yolu.clone() {
        dosyalar.push(yol);
    }
    match yayina_gonder(&kok, &dosyalar, logs) {
        Ok(adres) => {
            lead.yayin_linki = Some(adres.clone());
            lead.karne_linki = karne_yolu.map(|_| yayin_alt_adresi(&adres, &karne_dosyasi));
            if let Some(karne_adresi) = lead.karne_linki.clone() {
                logs.push(format!("[i] Dijital karne de yayında: {karne_adresi}"));
            }
            logs.push(format!("[✓] {} yayınlandı: {adres}", lead.name));
            Some(adres)
        }
        Err(hata) => {
            logs.push(format!("[!] Yayın başarısız: {hata}"));
            None
        }
    }
}

/// Demoyu GitHub Pages'e yayınlar, linki günlüğe yazar ve tarayıcıda açar.
fn demo_yayinla(lead: &mut BusinessLead, gonderen: &str, logs: &mut Vec<String>) {
    if let Some(adres) = yayina_gonder_ve_link_al(lead, gonderen, logs) {
        if let Err(hata) = tarayicida_ac(&adres) {
            logs.push(format!("[!] Link açılamadı: {hata}"));
        }
    }
}

/// Demos klasörünü dosya yöneticisinde açar.
fn klasoru_ac(yol: &str) -> Result<(), String> {
    let tam = fs::canonicalize(yol).unwrap_or_else(|_| std::path::PathBuf::from(yol));
    let hedef = tam.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    let sonuc = Command::new("explorer").arg(&hedef).spawn();
    #[cfg(target_os = "macos")]
    let sonuc = Command::new("open").arg(&hedef).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let sonuc = Command::new("xdg-open").arg(&hedef).spawn();
    sonuc
        .map(|_| ())
        .map_err(|hata| format!("{hedef} açılamadı: {hata}"))
}

fn tarayicida_ac(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let sonuc = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let sonuc = Command::new("open").arg(url).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let sonuc = Command::new("xdg-open").arg(url).spawn();
    sonuc
        .map(|_| ())
        .map_err(|hata| format!("{url} açılamadı: {hata}"))
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1120.0, 720.0])
            .with_title("Nexus Enterprise - İşletme Keşif ve Demo Otomasyonu"),
        ..Default::default()
    };

    eframe::run_native(
        "Nexus Enterprise",
        options,
        Box::new(|cc| {
            yazi_tipi_yukle(&cc.egui_ctx);
            tema_uygula(&cc.egui_ctx);
            Box::<LeadApp>::default()
        }),
    )
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn turkce_karakterler_url_kodlanir() {
        assert_eq!(
            url_encode("Beşiktaş, İstanbul"),
            "Be%C5%9Fikta%C5%9F%2C%20%C4%B0stanbul"
        );
    }

    #[test]
    fn web_sitesi_olan_isletmeler_atlanir() {
        let json = r#"{"elements":[
            {"type":"node","id":1,"lat":41.0,"lon":29.0,"tags":{"name":"Sitesiz Kafe","amenity":"cafe","phone":"+90 212 000 00 00"}},
            {"type":"node","id":2,"lat":41.1,"lon":29.1,"tags":{"name":"Siteli Kafe","amenity":"cafe","website":"https://ornek.com"}},
            {"type":"way","id":3,"center":{"lat":41.2,"lon":29.2},"tags":{"name":"Adresli Restoran","amenity":"restaurant","addr:street":"Barbaros Bulvarı","addr:housenumber":"12"}},
            {"type":"node","id":4,"tags":{"amenity":"cafe"}}
        ]}"#;

        let (liste, siteli) =
            overpass_isle(json, "İstanbul", "Beşiktaş", "Restoran").expect("yanıt işlenmeliydi");

        assert_eq!(siteli, 1, "web sitesi olan kayıt sayılmalı");
        assert_eq!(liste.len(), 2, "isimsiz kayıt listeye girmemeli");

        let kafe = liste
            .iter()
            .find(|isletme| isletme.name == "Sitesiz Kafe")
            .expect("sitesiz kafe listede olmalı");
        assert_eq!(kafe.phone, "+90 212 000 00 00");
        assert_eq!(kafe.address, "Beşiktaş / İstanbul");
        assert_eq!(kafe.kaynak, "https://www.openstreetmap.org/node/1");

        let restoran = liste
            .iter()
            .find(|isletme| isletme.name == "Adresli Restoran")
            .expect("adresli restoran listede olmalı");
        assert_eq!(
            restoran.address,
            "Barbaros Bulvarı No:12, Beşiktaş / İstanbul"
        );
    }

    fn ornek_isletme() -> BusinessLead {
        BusinessLead {
            name: "Kafe <Deneme>".to_string(),
            address: "Beşiktaş / İstanbul".to_string(),
            category: "Restoran".to_string(),
            phone: "+90 532 123 45 67".to_string(),
            selected: true,
            kaynak: "https://www.openstreetmap.org/node/42".to_string(),
            konum: Some((41.04285, 29.00753)),
            calisma_saatleri: "Mo-Fr 09:00-18:00; Sa 10:00-16:00".to_string(),
            kayit_dolulugu: 0,
            site: SiteDurumu::Yok,
            site_linki: String::new(),
            son_guncelleme_gun: Some(30),
            inceleme_sayisi: None,
            yildiz_puani: None,
            durum: musteriler::Durum::Yeni,
            demo_yolu: None,
            mesaj_hazir: false,
            yayin_linki: None,
            karne_linki: None,
            mesaj_varyanti: None,
        }
    }

    #[test]
    fn potansiyeli_yuksek_isletmeler_once_siralanir() {
        let mut telefonlu_saatli = ornek_isletme();
        telefonlu_saatli.name = "Zeynep Restoran".to_string();

        let mut telefonlu_a = ornek_isletme();
        telefonlu_a.name = "Işık Kafe".to_string();
        telefonlu_a.calisma_saatleri.clear();

        let mut telefonlu_b = ornek_isletme();
        telefonlu_b.name = "İnci Kafe".to_string();
        telefonlu_b.calisma_saatleri.clear();

        let mut saatli = ornek_isletme();
        saatli.name = "Ahmet Berber".to_string();
        saatli.phone.clear();

        let mut ciplak = ornek_isletme();
        ciplak.name = "Burak Kafe".to_string();
        ciplak.phone.clear();
        ciplak.calisma_saatleri.clear();

        let mut liste = vec![ciplak, saatli, telefonlu_a, telefonlu_b, telefonlu_saatli];
        leads_sirala(&mut liste);

        let adlar: Vec<&str> = liste.iter().map(|isletme| isletme.name.as_str()).collect();
        assert_eq!(
            adlar,
            vec![
                "Zeynep Restoran",
                "İnci Kafe",
                "Işık Kafe",
                "Ahmet Berber",
                "Burak Kafe"
            ],
            "telefon+saat > telefon > saat > hiçbiri; eşit puanda Türkçe alfabetik"
        );
    }

    #[test]
    fn kayit_dolulugu_ek_etiketlerle_artar() {
        let bos: serde_json::Value =
            serde_json::from_str(r#"{"name":"Kafe","amenity":"cafe"}"#).expect("json");
        assert_eq!(kayit_dolulugu(&bos), 0, "isim + tür etiketi puan getirmez");

        let dolu: serde_json::Value = serde_json::from_str(
            r#"{"name":"Kafe","amenity":"cafe","phone":"+90 212 000 00 00",
               "addr:street":"Cadde","cuisine":"turkish","wheelchair":"yes",
               "outdoor_seating":"yes","payment:credit_cards":"yes",
               "brand":"Deneme"}"#,
        )
        .expect("json");
        assert_eq!(
            kayit_dolulugu(&dolu),
            10,
            "puanlanan alanlar (telefon/adres) sayılmaz, diğer 5 etiket tavanı doldurur"
        );
    }

    #[test]
    fn potansiyel_puani_niyet_ve_ulasilabilirligi_puanlar() {
        let mut tam = ornek_isletme();
        tam.address = "Barbaros Bulvarı No:12, Beşiktaş / İstanbul".to_string();
        assert_eq!(
            potansiyel_puani(&tam),
            86,
            "site yok 12 + mobil telefon 30 + saat 8 + adres 8 + güncel 5 + sektör 20 + konum 3"
        );
        assert!(potansiyel_gerekcesi(&tam).starts_with("86/100 · mobil numara"));

        let mut sosyal = ornek_isletme();
        sosyal.address = "Barbaros Bulvarı No:12, Beşiktaş / İstanbul".to_string();
        sosyal.site = SiteDurumu::Sosyal;
        sosyal.site_linki = "https://instagram.com/kafe".to_string();
        assert_eq!(
            potansiyel_puani(&sosyal),
            100,
            "sosyal medyası olup sitesi olmayan en sıcak hedef (100'de tavanlanır)"
        );
        assert!(potansiyel_puani(&sosyal) > potansiyel_puani(&tam));
        assert!(potansiyel_gerekcesi(&sosyal).contains("sosyal medya var, site yok"));

        let mut incelemeli = ornek_isletme();
        incelemeli.site = SiteDurumu::Sosyal;
        incelemeli.inceleme_sayisi = Some(150);
        assert!(
            potansiyel_gerekcesi(&incelemeli).contains("150 Google incelemesi"),
            "inceleme sayısı gerekçede görünmeli"
        );

        let mut ciplak = ornek_isletme();
        ciplak.phone.clear();
        ciplak.calisma_saatleri.clear();
        ciplak.address = "Beşiktaş / İstanbul".to_string();
        ciplak.konum = None;
        ciplak.son_guncelleme_gun = None;
        assert_eq!(
            potansiyel_puani(&ciplak),
            32,
            "yalnızca 'site yok' + sektör puanı kalır"
        );
        assert!(potansiyel_puani(&ciplak) < SICAK_ESIK);
    }

    #[test]
    fn gecici_hatalar_ayirt_edilir() {
        assert!(gecici_hata("sunucu HTTP 502 döndü: bad gateway"));
        assert!(gecici_hata("sunucu HTTP 429 döndü: çok istek"));
        assert!(gecici_hata("bağlantı kurulamadı: dns"));
        assert!(!gecici_hata("sunucu HTTP 400 döndü: kötü istek"));
        assert!(!gecici_hata("konum yanıtı çözümlenemedi"));
        // Zaman aşımı da geçicidir: sunucu sırası yoğunken bağlantı düşer.
        assert!(gecici_hata("zaman aşımı: timed out reading response"));
    }

    #[test]
    fn retry_after_basligi_beklemeye_cevrilir() {
        let mesaj = "sunucu HTTP 429 döndü (Retry-After: 7 sn): çok istek";
        assert_eq!(retry_after_sn(mesaj), Some(7));
        // Aşırı uzun süreler kırpılır: tarama bir sunucuda kilitlenmemeli.
        assert_eq!(
            retry_after_sn("sunucu HTTP 429 döndü (Retry-After: 120 sn): yavaş"),
            Some(15)
        );
        // Başlık yoksa artan bekleme kullanılır.
        assert_eq!(retry_after_sn("sunucu HTTP 503 döndü: yoğun"), None);
    }
    /// Birincil sunucu 502 döndüğünde zincirin yedek sunucuya geçtiğini ve
    /// ilçenin verisiz kalmadığını doğrular (ağ gerekmez).
    #[test]
    fn overpass_birincil_502_verince_yedek_sunucuya_gecer() {
        let mut denemeler: Vec<String> = Vec::new();
        let mut gonder = |adres: &str| -> Result<String, String> {
            denemeler.push(adres.to_string());
            if adres.contains("birincil") {
                Err("sunucu HTTP 502 döndü: yoğun".to_string())
            } else {
                Ok("{\"elements\":[]}".to_string())
            }
        };
        let sunucular = [
            ("https://sahte-birincil/api/interpreter", "sahte-birincil"),
            ("https://sahte-yedek/api/interpreter", "sahte-yedek"),
        ];
        let (govde, sunucu_adi) =
            overpass_istek_zincir(&mut gonder, &sunucular, Duration::from_secs(20))
                .expect("yedek sunucudan yanıt gelmeliydi");

        assert_eq!(sunucu_adi, "sahte-yedek", "502 sonrası yedeğe geçilmeli");
        assert!(
            govde.contains("elements"),
            "yedek sunucunun gövdesi dönmeli"
        );
        assert_eq!(
            denemeler.len(),
            OVERPASS_DENEMESI + 1,
            "birincil {OVERPASS_DENEMESI} kez denenip yedeğe geçilmeli"
        );
    }

    #[test]
    fn overpass_tum_sunucular_dusunce_hepsini_raporlar() {
        let mut gonder =
            |_: &str| -> Result<String, String> { Err("sunucu HTTP 504 döndü".to_string()) };
        let sunucular = [
            ("https://a/api/interpreter", "a.example"),
            ("https://b/api/interpreter", "b.example"),
        ];
        let hata = overpass_istek_zincir(&mut gonder, &sunucular, Duration::from_secs(20))
            .expect_err("hiçbiri yanıt vermedi");
        // Kayıp ilçenin nedeni günlükte sunucu sunucu görünmeli.
        assert!(hata.contains("a.example"), "hata: {hata}");
        assert!(hata.contains("b.example"), "hata: {hata}");
    }

    #[test]
    fn overpass_sure_butcesi_asilinca_kalan_sunucu_denenmez() {
        let mut denemeler = 0usize;
        let mut gonder = |_: &str| -> Result<String, String> {
            denemeler += 1;
            Err("sunucu HTTP 429 döndü (Retry-After: 1 sn): çok istek".to_string())
        };
        let sunucular = [
            ("https://a/api/interpreter", "a.example"),
            ("https://b/api/interpreter", "b.example"),
        ];
        // Bütçe sıfır: hiçbir sunucuya gidilmemeli.
        let hata = overpass_istek_zincir(&mut gonder, &sunucular, Duration::ZERO)
            .expect_err("bütçe dolunca hata dönmeli");
        assert_eq!(denemeler, 0, "bütçe dolunca istek atılmamalı");
        assert!(hata.contains("süre bütçesi doldu"), "hata: {hata}");
    }

    #[test]
    fn overpass_sunuculari_uc_global_ayna_icerir() {
        assert_eq!(OVERPASS_SUNUCULARI.len(), 3);
        // Bölgesel ayna (yalnız İsviçre verisi) yedek olarak kullanılamaz:
        // kapsam dışı ilçede hata değil boş döner.
        assert!(OVERPASS_SUNUCULARI
            .iter()
            .all(|(_, ad)| *ad != "overpass.osm.ch"));
        assert!(OVERPASS_SUNUCULARI
            .iter()
            .all(|(adres, _)| adres.starts_with("https://")));
    }

    #[test]
    fn tekrar_dene_gecici_hatada_yeniden_dener() {
        let mut deneme = 0;
        let sonuc = tekrar_dene(|| {
            deneme += 1;
            if deneme < 2 {
                Err("sunucu HTTP 503 döndü: yoğun".to_string())
            } else {
                Ok(7)
            }
        });
        assert_eq!(sonuc, Ok(7));
        assert_eq!(deneme, 2, "ilk hata geçici olduğu için tekrar denenmeli");
    }

    #[test]
    fn tekrar_dene_kalici_hatada_durmaz() {
        let mut deneme = 0;
        let sonuc: Result<(), String> = tekrar_dene(|| {
            deneme += 1;
            Err("sunucu HTTP 400 döndü: kötü istek".to_string())
        });
        assert!(sonuc.is_err());
        assert_eq!(deneme, 1, "kalıcı hatada tekrar denenmemeli");
    }

    #[test]
    fn konum_onbellegi_kaydeder_ve_okur() {
        let mut onbellek = KonumOnbellek::default();
        assert!(onbellek.al("İstanbul", "Beşiktaş").is_none());

        onbellek.ekle(
            "İstanbul",
            "Beşiktaş",
            &Konum {
                enlem: 41.04,
                boylam: 29.0,
                yaricap_m: 3_000,
                alan_id: Some(3_601_765_893),
            },
        );
        let konum = onbellek
            .al("İstanbul", "Beşiktaş")
            .expect("önbellekten gelmeli");
        assert_eq!(konum.alan_id, Some(3_601_765_893));
        assert_eq!(konum.yaricap_m, 3_000);
        assert!(
            onbellek.al("istanbul", "besiktas").is_some(),
            "Türkçe karakter ve büyük/küçük harf farkı aynı kaydı bulmalı"
        );

        let metin = serde_json::to_string(&onbellek).expect("json");
        let geri: KonumOnbellek = serde_json::from_str(&metin).expect("json");
        assert!(geri.al("İstanbul", "Beşiktaş").is_some());

        // 180 günden eski kayıt geçersiz sayılır.
        let anahtar = KonumOnbellek::anahtar("İstanbul", "Beşiktaş");
        if let Some(kayit) = onbellek.kayitlar.get_mut(&anahtar) {
            kayit.tarih = "2020-01-01".to_string();
        }
        assert!(onbellek.al("İstanbul", "Beşiktaş").is_none());
    }

    #[test]
    fn google_istegi_page_size_kullanir() {
        let govde = google_govdesi("İstanbul", "Beşiktaş", "Restoran", None);
        assert_eq!(govde["pageSize"], SONUC_SINIRI);
        assert!(
            govde.get("maxResultCount").is_none(),
            "kullanımdan kaldırılan maxResultCount gönderilmemeli"
        );
        assert!(govde.get("pageToken").is_none(), "ilk sayfada jeton yok");
        assert_eq!(govde["textQuery"], "Restoran Beşiktaş İstanbul");

        let ikinci = google_govdesi("İstanbul", "Beşiktaş", "Restoran", Some("JETON"));
        assert_eq!(ikinci["pageToken"], "JETON");
        assert_eq!(ikinci["pageSize"], SONUC_SINIRI);
    }

    #[test]
    fn sonuc_onbellegi_ilce_sektor_bazinda_ayrisir() {
        let mut onbellek = SonucOnbellek::default();
        onbellek.ekle(
            "İstanbul",
            "Beşiktaş",
            "Kafe",
            &[ornek_isletme()],
            3,
            "overpass-api.de",
        );
        let kayit = onbellek
            .al("İstanbul", "Beşiktaş", "Kafe")
            .expect("önbellekte olmalı");
        assert_eq!(kayit.leads.len(), 1);
        assert_eq!(kayit.siteli, 3);
        assert_eq!(kayit.gun, 0);
        // Aynı ilçenin başka sektörü ve başka ilçe önbellekte yok.
        assert!(onbellek.al("İstanbul", "Beşiktaş", "Restoran").is_none());
        assert!(onbellek.al("İstanbul", "Kadıköy", "Kafe").is_none());
    }

    #[test]
    fn sonuc_onbelleginde_eski_kayit_kullanilmaz() {
        let mut onbellek = SonucOnbellek::default();
        onbellek.ekle(
            "İstanbul",
            "Beşiktaş",
            "Kafe",
            &[ornek_isletme()],
            0,
            "overpass-api.de",
        );
        let anahtar = SonucOnbellek::anahtar("İstanbul", "Beşiktaş", "Kafe");
        onbellek.kayitlar.get_mut(&anahtar).expect("kayıt").tarih = "2020-01-01".to_string();
        assert!(
            onbellek.al("İstanbul", "Beşiktaş", "Kafe").is_none(),
            "geçerlilik süresi dolmuş kayıt kullanılmamalı"
        );
    }

    #[test]
    fn onbellekli_ilce_ag_istemez() {
        // Önbellekte kayıt varsa ilçe taraması ağa hiç çıkmaz: imkânsız bir
        // konum ve gerçek ajanla bile sonuç önbellekten döner.
        let mut onbellek = SonucOnbellek::default();
        onbellek.ekle(
            "İstanbul",
            "Beşiktaş",
            "Restoran",
            &[ornek_isletme(), ornek_isletme()],
            2,
            "overpass-api.de",
        );
        let konum = Konum {
            enlem: 0.0,
            boylam: 0.0,
            yaricap_m: 1,
            alan_id: None,
        };
        let sonuc = ilce_taramasi(
            &http_ajani(),
            &mut onbellek,
            "İstanbul",
            "Beşiktaş",
            "Restoran",
            &konum,
        );
        match sonuc {
            IlceTaramasi::Onbellek(kayit) => {
                assert_eq!(kayit.leads.len(), 2);
                assert_eq!(kayit.gun, 0);
            }
            _ => panic!("sonuç önbellekten gelmeliydi, ağa çıkılmamalı"),
        }
    }

    #[test]
    fn sonuc_onbellegi_json_gidis_donusu() {
        let mut onbellek = SonucOnbellek::default();
        onbellek.ekle(
            "İstanbul",
            "Beşiktaş",
            "Kafe",
            &[ornek_isletme()],
            1,
            "z.overpass-api.de",
        );
        let metin = serde_json::to_string(&onbellek).expect("serileşmeli");
        let geri: SonucOnbellek = serde_json::from_str(&metin).expect("çözümlenmeli");
        let kayit = geri
            .al("İstanbul", "Beşiktaş", "Kafe")
            .expect("kayıt korunmalı");
        assert_eq!(kayit.leads[0].name, "Kafe <Deneme>");
        assert_eq!(kayit.leads[0].phone, "+90 532 123 45 67");
        assert_eq!(kayit.leads[0].site, SiteDurumu::Yok);
        assert_eq!(kayit.leads[0].durum, musteriler::Durum::Yeni);
        assert_eq!(kayit.leads[0].konum, Some((41.04285, 29.00753)));
    }

    #[test]
    fn gun_farki_tarihleri_cozer() {
        assert_eq!(gun_farki("2026-10-01", "2026-10-04"), Some(3));
        assert_eq!(gun_farki("2026-10-04", "2026-10-01"), Some(-3));
        assert_eq!(gun_farki("bozuk", "2026-10-01"), None);
        assert_eq!(gun_farki("2026-10-01", "2026-10-01"), Some(0));
    }

    #[test]
    fn kaynak_secimi_etiketlenir() {
        assert_eq!(Kaynak::HEPSI.len(), 3);
        assert_eq!(Kaynak::Hibrit.etiket(), "Hibrit");
        assert_eq!(Kaynak::Google.etiket(), "Google");
        assert_eq!(Kaynak::Osm.etiket(), "OSM");
        assert!(Kaynak::Google.aciklama().contains("inceleme"));
        assert!(Kaynak::Osm.aciklama().contains("ücretsiz"));
    }

    #[test]
    fn google_kaynaginda_inceleme_sayisi_once_gelir() {
        let mut cok_inceleme = ornek_isletme();
        cok_inceleme.name = "Zeynep Restoran".to_string();
        cok_inceleme.inceleme_sayisi = Some(140);

        let mut az_inceleme = ornek_isletme();
        az_inceleme.name = "Ahmet Kafe".to_string();
        az_inceleme.inceleme_sayisi = Some(12);

        let mut incelemesiz = ornek_isletme();
        incelemesiz.name = "Burak Berber".to_string();
        incelemesiz.inceleme_sayisi = None;

        let mut liste = vec![incelemesiz, az_inceleme, cok_inceleme];
        incelemeye_gore_sirala(&mut liste);

        let adlar: Vec<&str> = liste.iter().map(|isletme| isletme.name.as_str()).collect();
        assert_eq!(
            adlar,
            vec!["Zeynep Restoran", "Ahmet Kafe", "Burak Berber"],
            "inceleme sayısı azalan sırada olmalı"
        );
    }

    #[test]
    fn overpass_sorgusu_niyetli_gruba_ayri_kota_verir() {
        let sorgu = overpass_sorgusu(
            &[("amenity", "^restaurant$")],
            "(area.a)",
            Some(3_600_000_001),
        );
        assert!(sorgu.contains("area(3600000001)->.a;"), "{sorgu}");
        assert!(sorgu.contains(")->.sitesiz;"), "sitesiz kümesi olmalı");
        assert!(sorgu.contains(")->.niyetli;"), "niyetli kümesi olmalı");
        assert!(
            sorgu.contains(&format!(".sitesiz out center meta {SONUC_SINIRI};")),
            "sitesiz grubun kotası: {sorgu}"
        );
        assert!(
            sorgu.contains(&format!(".niyetli out center meta {NIYETLI_KOTA};")),
            "niyetli grubun kotası: {sorgu}"
        );
        assert!(
            !sorgu.contains("(\n("),
            "küme bloğu iki kez açılmamalı (Overpass 400 verir): {sorgu}"
        );
        assert!(
            sorgu.contains("[\"contact:website\"~\"instagram"),
            "sosyal desen contact:website etiketi için de aranmalı"
        );
    }

    #[test]
    fn sosyal_medya_ve_ucretsiz_site_ayirt_edilir() {
        for sosyal in [
            "https://www.instagram.com/kafedeneme",
            "http://facebook.com/kafe",
            "https://linktr.ee/kafe",
            "https://wa.me/905321234567",
        ] {
            assert_eq!(
                site_durumu_bul(sosyal),
                Some(SiteDurumu::Sosyal),
                "{sosyal} sosyal medya sayılmalı"
            );
        }
        for ucretsiz in [
            "https://kafe.business.site",
            "https://kafedeneme.blogspot.com",
            "https://kafe.wixsite.com/anasayfa",
        ] {
            assert_eq!(
                site_durumu_bul(ucretsiz),
                Some(SiteDurumu::Ucretsiz),
                "{ucretsiz} ücretsiz site sayılmalı"
            );
        }
        assert_eq!(site_durumu_bul("https://kafedeneme.com"), None);
        assert_eq!(site_durumu_bul(""), None);
        assert_eq!(
            site_durumu_bul("https://box.com/kafe"),
            None,
            "x.com kuralı box.com gibi alan adlarını yakalamamalı"
        );
    }

    #[test]
    fn sosyal_medyali_kayit_listeye_girer() {
        let json = r#"{"elements":[
            {"type":"node","id":10,"lat":41.0,"lon":29.0,"timestamp":"2026-09-01T12:00:00Z","tags":{"name":"Instagram Kafe","amenity":"cafe","website":"https://instagram.com/instagramkafe"}},
            {"type":"node","id":11,"lat":41.0,"lon":29.1,"tags":{"name":"Site Sahibi","amenity":"cafe","website":"https://kafesitesi.com"}},
            {"type":"node","id":12,"lat":41.0,"lon":29.2,"tags":{"name":"Facebook Berber","shop":"hairdresser","contact:facebook":"https://facebook.com/berber"}}
        ]}"#;

        let (liste, siteli) =
            overpass_isle(json, "İstanbul", "Beşiktaş", "Kafe").expect("yanıt işlenmeliydi");
        assert_eq!(siteli, 1, "gerçek sitesi olan kayıt ayıklanmalı");
        assert_eq!(liste.len(), 2, "sosyal medyalı iki kayıt listede kalmalı");
        assert!(liste
            .iter()
            .all(|isletme| isletme.site == SiteDurumu::Sosyal));

        let instagram = liste
            .iter()
            .find(|isletme| isletme.name == "Instagram Kafe")
            .expect("instagram kaydı listede olmalı");
        assert_eq!(instagram.site_linki, "https://instagram.com/instagramkafe");
        assert!(
            instagram.son_guncelleme_gun.is_some(),
            "OSM timestamp gün sayısına çevrilmeli"
        );
    }

    #[test]
    fn whatsapp_numarasi_uluslararasi_bicime_cevrilir() {
        assert_eq!(
            whatsapp_numarasi("+90 532 123 45 67").as_deref(),
            Some("905321234567")
        );
        assert_eq!(
            whatsapp_numarasi("0532 123 45 67").as_deref(),
            Some("905321234567")
        );
        assert_eq!(
            whatsapp_numarasi("532 123 45 67").as_deref(),
            Some("905321234567")
        );
        assert_eq!(
            whatsapp_numarasi("0212 444 62 62").as_deref(),
            Some("902124446262")
        );
        assert!(whatsapp_numarasi("telefon yok").is_none());
    }

    #[test]
    fn demo_dosyasi_adi_turkce_karakterleri_sadeler() {
        assert_eq!(dosya_slug("Şişli Çiçekçi & Kahve"), "sisli-cicekci-kahve");
    }

    #[test]
    fn demo_html_paylasim_etiketleri_icerir() {
        let html = demo_html(&ornek_isletme(), "+90 500 111 22 33");
        assert!(html.contains("<meta property=\"og:title\""), "{html}");
        assert!(html.contains("og:description"));
        assert!(html.contains("og:locale\" content=\"tr_TR"));
        assert!(html.contains("name=\"description\""));
        // Başlık işletme adını ve sektörü taşır; kaçışlama korunur.
        assert!(html.contains("og:site_name\" content=\"Kafe &lt;Deneme&gt;"));
    }

    #[test]
    fn takip_mesaji_gun_sayisini_ve_linki_yazar() {
        let mut kayit = musteriler::Musteri {
            anahtar: "osm:node/1".to_string(),
            ad: "Moda Kafe".to_string(),
            telefon: "+90 532 123 45 67".to_string(),
            kategori: "Kafe".to_string(),
            ..musteriler::Musteri::default()
        };
        let bugun = bugun_anahtari();
        kayit.son_gonderim = Some(
            chrono::Local::now()
                .date_naive()
                .checked_sub_days(chrono::Days::new(5))
                .map(|gun| gun.format("%Y-%m-%d").to_string())
                .unwrap_or(bugun.clone()),
        );
        kayit.yayin_linki = Some("https://ornek.github.io/r/demos/kafe-1.html".to_string());
        let mesaj = takip_mesaji(&kayit);
        assert!(mesaj.contains("Moda Kafe"), "{mesaj}");
        assert!(mesaj.contains("5 gün önce"), "{mesaj}");
        assert!(mesaj.contains("https://ornek.github.io/r/demos/kafe-1.html"));
        assert!(mesaj.contains("RET"), "mesajın çıkış yolu olmalı");

        // Yayın linki yoksa mesaj linksiz ama tutarlı kalır.
        kayit.yayin_linki = None;
        let linksiz = takip_mesaji(&kayit);
        assert!(!linksiz.contains("http"), "{linksiz}");
        assert!(linksiz.contains("Moda Kafe"));
    }

    #[test]
    fn demo_html_ozel_karakterleri_kacirir_ve_tel_ekler() {
        let html = demo_html(&ornek_isletme(), "+90 500 111 22 33");
        assert!(html.contains("Kafe &lt;Deneme&gt;"));
        assert!(html.contains("tel:+905321234567"));
        assert!(html.contains("Demo süresi: 12 saat"));
        assert!(
            html.contains("Menü"),
            "restoran demosunda menü bölümü olmalı"
        );
        assert!(
            html.contains("export/embed.html") && html.contains("marker=41.04285,29.00753"),
            "konum varsa gömülü harita olmalı"
        );
        assert!(html.contains("Pzt-Cum 09:00-18:00 · Cmt 10:00-16:00"));
        assert!(
            html.contains("id=\"acik-durum\""),
            "açık/kapalı satırı olmalı"
        );
        assert!(
            html.contains("const program ="),
            "haftalık program JS'e gömülmeli"
        );
        assert!(
            html.contains("setInterval(durumGuncelle, 60000)"),
            "gösterge dakikada bir güncellenmeli"
        );
        assert!(
            html.contains("kalanMetni"),
            "kapanışa kalan süre hesaplanmalı"
        );
        assert!(
            html.contains("\" gün\"") && html.contains("/ 1440"),
            "bir günden uzun süreler gün olarak yazılmalı"
        );
        assert!(
            html.contains("Kapanmak üzere") && html.contains("text-amber-400"),
            "kapanışa 15 dk kala turuncu uyarı olmalı"
        );

        let mut konumsuz = ornek_isletme();
        konumsuz.konum = None;
        konumsuz.calisma_saatleri = String::new();
        let sade = demo_html(&konumsuz, "");
        assert!(
            !sade.contains("export/embed.html"),
            "konum yoksa harita eklenmemeli"
        );
        assert!(!sade.contains("🕒"), "saat yoksa satır eklenmemeli");
    }

    #[test]
    fn calisma_saatleri_turkceye_cevrilir() {
        assert_eq!(
            calisma_saatleri_metni("Mo-Fr 09:00-18:00; Sa 10:00-16:00"),
            "Pzt-Cum 09:00-18:00 · Cmt 10:00-16:00"
        );
        assert_eq!(calisma_saatleri_metni("24/7"), "24 saat açık");
        assert_eq!(calisma_saatleri_metni("Su off"), "Paz kapalı");
        assert_eq!(calisma_saatleri_metni(""), "");
        assert_eq!(
            calisma_saatleri_metni("Mo-Su 10:00-22:00 \"Sadece randevu\""),
            "Pzt-Paz 10:00-22:00 \"Sadece randevu\""
        );
    }

    #[test]
    fn whatsapp_durumu_acik_kapali_cumlesi() {
        let program = haftalik_program("Mo-Fr 09:00-18:00; Sa 10:00-16:00; Su off")
            .expect("program çözülmeli");
        assert_eq!(
            durum_cumlesi(&program, 0, 12 * 60),
            "İşletmeniz şu an açık (kapanış 18:00)."
        );
        assert_eq!(
            durum_cumlesi(&program, 0, 20 * 60),
            "İşletmeniz şu an kapalı; açılış yarın 09:00."
        );
        assert_eq!(
            durum_cumlesi(&program, 5, 20 * 60),
            "İşletmeniz şu an kapalı; açılış Pazartesi 09:00."
        );
        assert_eq!(
            durum_cumlesi(&program, 6, 12 * 60),
            "İşletmeniz şu an kapalı; açılış yarın 09:00."
        );

        let her_gun = haftalik_program("24/7").expect("program çözülmeli");
        assert_eq!(
            durum_cumlesi(&her_gun, 3, 0),
            "İşletmeniz şu an açık (24 saat)."
        );
    }

    #[test]
    fn panel_durum_cumlesi_kisaltilir() {
        assert_eq!(
            durum_kisa_cumle("İşletmeniz şu an açık (kapanış 18:00)."),
            "Şu an açık (kapanış 18:00)"
        );
        assert_eq!(
            durum_kisa_cumle("İşletmeniz şu an kapalı; açılış yarın 09:00."),
            "Şu an kapalı; açılış yarın 09:00"
        );
    }

    #[test]
    fn whatsapp_mesaji_durumu_icerir() {
        let mesaj = whatsapp_mesaji_varyant(
            &ornek_isletme(),
            Some("İşletmeniz şu an kapalı; açılış yarın 09:00."),
            musteriler::Varyant::A,
        );
        assert!(mesaj.contains("İşletmeniz şu an kapalı; açılış yarın 09:00."));
        assert!(mesaj.contains("demoda menü ve günün yemekleri"));

        let durumsuz = whatsapp_mesaji_varyant(&ornek_isletme(), None, musteriler::Varyant::A);
        assert!(!durumsuz.contains("İşletmeniz şu an"));
        assert!(durumsuz.contains("12 saatlik demo dosyasını"));
        assert!(durumsuz.contains("demoda menü ve günün yemekleri"));
    }

    #[test]
    fn whatsapp_mesaji_yayin_linkini_icerir() {
        let mut lead = ornek_isletme();
        let linksiz = whatsapp_mesaji(&lead);
        assert!(
            !linksiz.contains("http"),
            "link yokken adres yazılmamalı: {linksiz}"
        );
        assert!(linksiz.contains("demo dosyasını ekliyorum"));

        lead.yayin_linki = Some("https://ornek.github.io/repo/demos/kafe-42.html".to_string());
        let linkli = whatsapp_mesaji(&lead);
        assert!(linkli.contains("https://ornek.github.io/repo/demos/kafe-42.html"));
        assert!(
            !linkli.contains("dosyasını ekliyorum"),
            "link varken dosya gönderme cümlesi kalmamalı"
        );
        // Durum bilgisi linkle birlikte de korunur.
        let with_durum = whatsapp_mesaji_varyant(
            &lead,
            Some("İşletmeniz şu an açık."),
            musteriler::Varyant::A,
        );
        assert!(with_durum.contains("İşletmeniz şu an açık."));
        assert!(with_durum.contains("kafe-42.html"));
        // WhatsApp bağlantısı için kodlama adresi bozmamalı.
        assert!(url_encode(&linkli).contains("https%3A%2F%2Fornek.github.io"));
    }

    #[test]
    fn whatsapp_mesaji_sektore_gore_kisisellesir() {
        let mut lead = ornek_isletme();
        lead.category = "Kuaför".to_string();
        let kuafor = whatsapp_mesaji(&lead);
        assert!(kuafor.contains("fotoğraf galerisi"), "{kuafor}");
        assert!(kuafor.contains("randevu"));

        lead.category = "Restoran".to_string();
        let restoran = whatsapp_mesaji(&lead);
        assert!(restoran.contains("menü"), "{restoran}");
        assert!(!restoran.contains("galeri"));

        lead.category = "Diş Kliniği".to_string();
        let klinik = whatsapp_mesaji(&lead);
        assert!(klinik.contains("hizmetler"), "{klinik}");
        assert!(klinik.contains("uzman kadro"));

        // Üç sektör için üç farklı cümle üretilmeli.
        assert_ne!(kuafor, restoran);
        assert_ne!(restoran, klinik);
        assert_ne!(kuafor, klinik);
    }

    #[test]
    fn whatsapp_gerekcesi_kendi_verisini_kullanir() {
        // Google kaydı: yıldız + yorum var, çalışma saati yok, yalnızca sosyal medya.
        let mut lead = ornek_isletme();
        lead.site = SiteDurumu::Sosyal;
        lead.inceleme_sayisi = Some(37);
        lead.yildiz_puani = Some(4.2);
        lead.calisma_saatleri.clear();
        let gerekce = whatsapp_gerekcesi(&lead).expect("gerekçe yazılmalı");
        assert!(
            gerekce.contains("Google'da 4,2★ ve 37 yorumunuz var"),
            "{gerekce}"
        );
        assert!(gerekce.contains("sosyal medya dışında siteniz yok"));
        assert!(gerekce.contains("çalışma saatiniz de yok"));
        assert!(gerekce.ends_with('.'));

        // OSM kaydı: yorum/puan yok, çalışma saati var → yalnızca site eksikliği.
        let osm = ornek_isletme();
        let gerekce = whatsapp_gerekcesi(&osm).expect("gerekçe yazılmalı");
        assert_eq!(gerekce, "Web siteniz yok.");

        // Yalnızca yorum sayısı varsa yıldız uydurulmaz.
        let mut yorumlu = ornek_isletme();
        yorumlu.calisma_saatleri.clear();
        yorumlu.site = SiteDurumu::Yok;
        yorumlu.inceleme_sayisi = Some(12);
        let gerekce = whatsapp_gerekcesi(&yorumlu).expect("gerekçe yazılmalı");
        assert!(gerekce.contains("Google'da 12 yorumunuz var"), "{gerekce}");
        assert!(!gerekce.contains('★'));
    }

    #[test]
    fn kisa_adres_sokak_yerine_mahalle_gosterir() {
        assert_eq!(
            kisa_adres("Moda Cad. No:12, Caferağa, Kadıköy / İstanbul"),
            "Caferağa, Kadıköy / İstanbul"
        );
        assert_eq!(
            kisa_adres("Barbaros Bulvarı No:12, Beşiktaş / İstanbul"),
            "Beşiktaş / İstanbul"
        );
        assert_eq!(
            kisa_adres("Caferağa, Kadıköy / İstanbul"),
            "Caferağa, Kadıköy / İstanbul"
        );
        assert_eq!(kisa_adres("Kadıköy / İstanbul"), "Kadıköy / İstanbul");
        assert_eq!(kisa_adres("  "), "");
        // Mahalle adı cadde/sokak kelimesiyle karışmamalı.
        assert_eq!(
            kisa_adres("Caddebostan, Kadıköy / İstanbul"),
            "Caddebostan, Kadıköy / İstanbul"
        );
    }

    #[test]
    fn varyant_b_soru_sormadan_gerekceyle_acar() {
        let mut lead = ornek_isletme();
        lead.yayin_linki = Some("https://ornek.github.io/r/demos/x.html".to_string());
        lead.inceleme_sayisi = Some(20);
        lead.yildiz_puani = Some(4.5);
        let a = whatsapp_mesaji_varyant(&lead, None, musteriler::Varyant::A);
        let b = whatsapp_mesaji_varyant(&lead, None, musteriler::Varyant::B);

        assert!(a.contains("ister misiniz?"), "{a}");
        assert!(!b.contains("ister misiniz?"), "{b}");
        // B selamdan hemen sonra veriyi verir; soru sormaz.
        assert!(
            b.contains("(Beşiktaş / İstanbul). Google'da 4,5★ ve 20 yorumunuz var"),
            "{b}"
        );
        assert!(b.contains("taslağı hazırladım"), "{b}");
        assert_ne!(a, b);
        // Ortak kapanış ve tek link korunur.
        for mesaj in [&a, &b] {
            assert!(mesaj.contains("\"RET\" yazın"));
            assert_eq!(mesaj.matches("http").count(), 1, "{mesaj}");
        }
    }

    #[test]
    fn mesaj_varyanti_kayittan_gelir() {
        let mut lead = ornek_isletme();
        lead.mesaj_varyanti = Some(musteriler::Varyant::B);
        assert!(!whatsapp_mesaji(&lead).contains("ister misiniz?"));

        lead.mesaj_varyanti = None;
        assert!(whatsapp_mesaji(&lead).contains("ister misiniz?"));
    }

    #[test]
    fn whatsapp_mesaji_sokak_adresi_yazmaz() {
        let mut lead = ornek_isletme();
        lead.address = "Moda Cad. No:12, Caferağa, Kadıköy / İstanbul".to_string();
        let mesaj = whatsapp_mesaji(&lead);
        assert!(mesaj.contains("(Caferağa, Kadıköy / İstanbul)"), "{mesaj}");
        assert!(!mesaj.contains("No:12"), "{mesaj}");
        assert!(!mesaj.contains("Moda Cad."), "{mesaj}");
    }

    #[test]
    fn whatsapp_mesaji_gerekceyi_baslangica_koyar() {
        let mut lead = ornek_isletme();
        lead.inceleme_sayisi = Some(41);
        lead.yildiz_puani = Some(4.6);
        let mesaj = whatsapp_mesaji(&lead);
        // Gerekçe, sorudan hemen sonra ve durum cümlesinden önce gelmeli.
        let soru = mesaj.find("ister misiniz?").expect("soru cümlesi");
        let gerekce = mesaj.find("4,6★ ve 41 yorumunuz var").expect("gerekçe");
        let taslak = mesaj
            .find("Taslak:")
            .or_else(|| mesaj.find("demo dosyasını"))
            .expect("taslak");
        assert!(soru < gerekce && gerekce < taslak, "{mesaj}");
        assert!(!mesaj.contains("  "), "çift boşluk olmamalı: {mesaj}");
    }

    #[test]
    fn yayin_alt_adresi_ayni_klasoru_korur() {
        assert_eq!(
            yayin_alt_adresi(
                "https://ornek.github.io/repo/demos/kafe-42.html",
                "karne-kafe-42.html"
            ),
            "https://ornek.github.io/repo/demos/karne-kafe-42.html"
        );
        // Klasör ayracı olmayan adres olduğu gibi kalır.
        assert_eq!(yayin_alt_adresi("adres-yok", "x.html"), "adres-yok");
    }

    #[test]
    fn karne_yoksa_yayindan_once_uretilir() {
        let klasor =
            std::env::temp_dir().join(format!("nexus_karne_hazirla_{}", std::process::id()));
        let klasor_yolu = klasor.to_string_lossy().to_string();
        let lead = ornek_isletme();
        let beklenen = klasor.join("karne-kafe-deneme-42.html");
        assert!(!beklenen.exists(), "test temiz başlamalı");

        let mut logs = Vec::new();
        let yol = karne_hazirla(&klasor_yolu, &lead, &mut logs).expect("karne üretilmeli");
        assert!(yol.ends_with("karne-kafe-deneme-42.html"), "{yol}");
        let icerik = fs::read_to_string(&yol).expect("karne okunmalı");
        assert!(icerik.contains("Dijital Görünürlük Karnesi"));
        assert!(logs.is_empty(), "beklenmeyen günlük: {logs:?}");

        // İkinci çağrı dosyayı yeniden üretmez, mevcut yolu döndürür.
        assert_eq!(karne_hazirla(&klasor_yolu, &lead, &mut logs), Some(yol));

        let _ = fs::remove_dir_all(&klasor);
    }

    #[test]
    fn whatsapp_mesaji_tek_link_tasir() {
        let mut lead = ornek_isletme();
        lead.yayin_linki = Some("https://ornek.github.io/repo/demos/kafe-42.html".to_string());
        lead.karne_linki = Some(yayin_alt_adresi(
            lead.yayin_linki.as_deref().unwrap_or_default(),
            "karne-kafe-42.html",
        ));
        let mesaj = whatsapp_mesaji(&lead);
        assert!(mesaj.contains("https://ornek.github.io/repo/demos/kafe-42.html"));
        assert!(
            !mesaj.contains("karne-kafe-42.html"),
            "karne ayrı adres olarak yazılmamalı: {mesaj}"
        );
        // Tek link: WhatsApp'ta okunabilirlik için sınır.
        assert_eq!(mesaj.matches("http").count(), 1, "{mesaj}");
    }

    #[test]
    fn karne_rozeti_demoya_eklenir_ve_tekrarlanmaz() {
        let klasor =
            std::env::temp_dir().join(format!("nexus_karne_rozeti_{}", std::process::id()));
        fs::create_dir_all(&klasor).expect("klasör oluşmalı");
        let demo = klasor.join("kafe.html");
        fs::write(&demo, "<html><body><h1>Kafe</h1></body></html>").expect("demo yazılmalı");
        let yol = demo.to_string_lossy().to_string();

        karne_rozeti_ekle(&yol, "karne-kafe.html").expect("rozet eklenmeli");
        let icerik = fs::read_to_string(&demo).expect("demo okunmalı");
        assert_eq!(icerik.matches("karne-kafe.html").count(), 1, "{icerik}");
        // Bağlantı gövdenin sonunda, mevcut içeriği bölmeden durur.
        assert!(icerik.find("</h1>").expect("başlık") < icerik.find("karne-kafe.html").unwrap());
        assert!(icerik.contains("</body>"));

        // Aynı demo yeniden yayınlanırsa ikinci rozet eklenmez.
        karne_rozeti_ekle(&yol, "karne-kafe.html").expect("ikinci çağrı sorunsuz olmalı");
        let tekrar = fs::read_to_string(&demo).expect("demo okunmalı");
        assert_eq!(tekrar.matches("karne-kafe.html").count(), 1, "{tekrar}");

        let _ = fs::remove_dir_all(&klasor);
    }

    #[test]
    fn haftalik_program_osm_saatlerini_cozer() {
        let program = haftalik_program("Mo-Fr 09:00-18:00; Sa 10:00-16:00; Su off")
            .expect("basit kurallar çözülmeli");
        assert_eq!(program[0], vec![(540, 1080)]);
        assert_eq!(program[4], vec![(540, 1080)]);
        assert_eq!(program[5], vec![(600, 960)]);
        assert!(program[6].is_empty(), "Su off -> kapalı");

        let gece = haftalik_program("Fr 20:00-02:00").expect("gece aralığı çözülmeli");
        assert_eq!(gece[4], vec![(1200, 1440)]);
        assert_eq!(gece[5], vec![(0, 120)]);

        let her_gun = haftalik_program("24/7").expect("24/7 çözülmeli");
        assert!(her_gun.iter().all(|gun| gun == &vec![(0, 1440)]));

        assert!(haftalik_program("Mo-Fr sunrise-sunset").is_none());
        assert!(haftalik_program("").is_none());

        let mut belirsiz = ornek_isletme();
        belirsiz.calisma_saatleri = "Mo-Fr sunrise-sunset".to_string();
        assert!(
            !demo_html(&belirsiz, "").contains("id=\"acik-durum\""),
            "çözülemeyen saatlerde gösterge eklenmemeli"
        );
    }

    #[test]
    fn demo_bolumu_sektore_gore_degisir() {
        let restoran = demo_bolumu("Restoran");
        assert!(restoran.contains("Menü") && restoran.contains("Günün Çorbası"));

        let kuaför = demo_bolumu("Kuaför");
        assert!(kuaför.contains("Galeri") && kuaför.contains("Saç Kesimi"));

        let klinik = demo_bolumu("Diş Kliniği");
        assert!(klinik.contains("Hizmetlerimiz") && klinik.contains("Randevu"));

        let firin = demo_bolumu("Fırın");
        assert!(firin.contains("Ürünlerimiz") && firin.contains("Taze Üretim"));

        let otel = demo_bolumu("Pansiyon");
        assert!(otel.contains("Odalar ve Konaklama"));

        assert_ne!(restoran, kuaför);
        assert_ne!(restoran, firin);
        assert_ne!(kuaför, klinik);
    }

    #[test]
    fn demo_dosyasi_gercekten_yazilir() {
        let klasor = std::env::temp_dir().join(format!(
            "nexus_demo_test_{}_{}",
            std::process::id(),
            dosya_slug("Kafe <Deneme>")
        ));
        let klasor_yolu = klasor.to_string_lossy().to_string();

        let yol = demo_yaz(&klasor_yolu, &ornek_isletme(), "+90 500 111 22 33")
            .expect("demo dosyası yazılmalı");
        assert!(
            yol.ends_with("kafe-deneme-42.html"),
            "beklenmeyen dosya: {yol}"
        );
        let icerik = fs::read_to_string(&yol).expect("yazılan dosya okunmalı");
        assert!(icerik.contains("Kafe &lt;Deneme&gt;"));
        // Karne demoyla birlikte üretilir ve demoda ona bağlantı vardır.
        let karne = klasor.join("karne-kafe-deneme-42.html");
        assert!(karne.exists(), "karne dosyası yazılmalı: {karne:?}");
        assert!(
            icerik.contains("href=\"karne-kafe-deneme-42.html\""),
            "demoda karne bağlantısı olmalı"
        );
        let karne_icerik = fs::read_to_string(&karne).expect("karne okunmalı");
        assert!(karne_icerik.contains("Dijital Görünürlük Karnesi"));
        assert!(karne_icerik.contains("Kafe &lt;Deneme&gt;"));
        assert!(
            karne_icerik.contains("href=\"kafe-deneme-42.html\""),
            "karneden demoya bağlantı olmalı"
        );

        let _ = fs::remove_dir_all(&klasor);
    }

    #[test]
    fn github_pages_adresi_uzak_adresten_uretilir() {
        for uzak in [
            "https://github.com/profesyonelofis-oss/final100.git",
            "https://github.com/profesyonelofis-oss/final100",
            "https://github.com/profesyonelofis-oss/final100/\n",
            "git@github.com:profesyonelofis-oss/final100.git",
        ] {
            let (sahip, depo) = github_depo(uzak).expect("depo çözülmeli");
            assert_eq!(sahip, "profesyonelofis-oss", "uzak: {uzak}");
            assert_eq!(depo, "final100", "uzak: {uzak}");
            assert_eq!(
                yayin_adresi(&sahip, &depo, "kafe-42.html"),
                "https://profesyonelofis-oss.github.io/final100/demos/kafe-42.html"
            );
        }
        // GitHub dışı ve eksik adresler sessizce link üretmemeli.
        assert!(github_depo("https://gitlab.com/a/b.git").is_none());
        assert!(github_depo("https://github.com/sadece-sahip").is_none());
        assert!(github_depo("").is_none());
    }

    #[test]
    fn yayin_dizini_dosyalari_sirali_listeler() {
        let dizin = yayin_dizini(&[
            "b-2.html".to_string(),
            "a-1.html".to_string(),
            "karne-a-1.html".to_string(),
        ]);
        assert!(dizin.contains("demos/a-1.html"));
        assert!(dizin.contains("demos/b-2.html"));
        assert!(dizin.contains("demos/karne-a-1.html"));
        assert!(
            dizin.find("a-1.html").unwrap() < dizin.find("b-2.html").unwrap(),
            "dosyalar sıralı olmalı"
        );
        // Karne öneki ve tireler başlıkta sadeleşir.
        assert!(dizin.contains(">a 1<"), "{}", dizin);
    }

    #[test]
    fn karne_eksik_site_ve_saati_puanlar() {
        // Örnek işletmede telefon var, site yok (40), çalışma saati var (0),
        // Google yorum/puan verisi yok (15) → 55 kayıp, yani 45 puan.
        let (puan, satirlar) = karne_olc(&ornek_isletme());
        assert_eq!(puan, 45);
        assert_eq!(karne_sinifi(puan).0, "Zayıf");
        assert_eq!(satirlar.len(), 4);
        let site = &satirlar[0];
        assert_eq!(site.baslik, "Web sitesi");
        assert_eq!(site.durum, KarneDurumu::Eksik);
        assert!(site.deger.contains("Web sitesi yok"));
        let saat = &satirlar[1];
        assert_eq!(saat.kayip, 0);
        assert_eq!(saat.durum, KarneDurumu::Iyi);
    }

    #[test]
    fn karne_google_yorum_ve_puanini_gosterir() {
        let mut lead = ornek_isletme();
        lead.inceleme_sayisi = Some(120);
        lead.yildiz_puani = Some(4.6);
        let (puan, satirlar) = karne_olc(&lead);
        // Yalnızca site eksik: 100 - 40 = 60.
        assert_eq!(puan, 60);
        let yorum = satirlar
            .iter()
            .find(|satir| satir.baslik == "Google yorumları")
            .expect("yorum satırı olmalı");
        assert!(yorum.deger.contains("120 yorum"), "{}", yorum.deger);
        assert!(yorum.deger.contains("4,6 ★"), "{}", yorum.deger);
        assert_eq!(yorum.kayip, 0);

        // Düşük puan, yorum sayısı çok olsa da ceza alır.
        let mut dusuk = ornek_isletme();
        dusuk.inceleme_sayisi = Some(80);
        dusuk.yildiz_puani = Some(3.1);
        let (_, satirlar) = karne_olc(&dusuk);
        let yorum = satirlar
            .iter()
            .find(|satir| satir.baslik == "Google yorumları")
            .expect("yorum satırı olmalı");
        assert_eq!(yorum.kayip, 4);
        assert!(yorum.deger.contains("3,1 ★"), "{}", yorum.deger);
    }

    #[test]
    fn karne_html_eksikleri_onerileri_ve_taslak_baglayicisini_yazar() {
        let lead = ornek_isletme();
        let html = karne_html(&lead, None);
        assert!(html.contains("Dijital Görünürlük Karnesi"));
        assert!(html.contains("Web sitesi yok"));
        // Metin HTML kaçışından geçer: kesme işareti `&#39;` olur.
        assert!(html.contains("Google&#39;da yorum ve puan verisi yok"));
        assert!(html.contains("Önerilen adımlar"));
        assert!(!html.contains("Taslağı aç"));
        // Pazarlama metni değil, kaydın kendi verisi yazılır; ad kaçırılır.
        assert!(html.contains("Kafe &lt;Deneme&gt;"));
        assert!(html.contains("İşletme kaydı") || html.contains("Kaynak:"));

        let html = karne_html(&lead, Some("kafe-deneme-42.html"));
        assert!(html.contains("href=\"kafe-deneme-42.html\""));
        assert!(html.contains("Taslağı aç"));
    }

    /// Canlı ağ isteği: `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore = "OpenStreetMap'e gerçek istek atar"]
    fn canli_ikinci_tarama_onbellekten_gelir() {
        let ajan = http_ajani();
        let konum = konum_bul(&ajan, "İstanbul", "Beşiktaş").expect("konum bulunmalı");
        // Boş önbellekle başlanır: ilk tarama ağa çıkmak zorunda.
        let mut onbellek = SonucOnbellek::default();

        let basla = Instant::now();
        let birinci = ilce_taramasi(
            &ajan,
            &mut onbellek,
            "İstanbul",
            "Beşiktaş",
            "Restoran",
            &konum,
        );
        let ilk_sure = basla.elapsed();
        let adet = match &birinci {
            IlceTaramasi::Ag { leads, sunucu, .. } => {
                println!(
                    "ilk tarama: {ilk_sure:?} · sunucu: {sunucu} · {} kayıt",
                    leads.len()
                );
                leads.len()
            }
            IlceTaramasi::Onbellek(_) => panic!("boş önbellekte ağa çıkmalıydı"),
            IlceTaramasi::Hata(hata) => panic!("tarama başarısız: {hata}"),
        };

        let basla = Instant::now();
        let ikinci = ilce_taramasi(
            &ajan,
            &mut onbellek,
            "İstanbul",
            "Beşiktaş",
            "Restoran",
            &konum,
        );
        let ikinci_sure = basla.elapsed();
        match &ikinci {
            IlceTaramasi::Onbellek(kayit) => {
                println!(
                    "önbellekli tarama: {ikinci_sure:?} · {} kayıt · {} gün önce",
                    kayit.leads.len(),
                    kayit.gun
                );
                assert_eq!(kayit.leads.len(), adet, "önbellek aynı listeyi vermeli");
            }
            _ => panic!("ikinci tarama önbellekten gelmeliydi, Overpass'a gitmemeli"),
        }
        assert!(
            ikinci_sure < ilk_sure,
            "önbellekli tarama ağdan hızlı olmalı ({ikinci_sure:?} < {ilk_sure:?})"
        );
    }

    #[test]
    #[ignore = "OpenStreetMap'e gerçek istek atar"]
    fn canli_tarama_gercek_isletme_dondurur() {
        let ajan = http_ajani();
        let konum = konum_bul(&ajan, "İstanbul", "Beşiktaş").expect("konum bulunmalı");
        assert!(konum.yaricap_m >= 1_200);
        println!("alan_id: {:?}", konum.alan_id);
        let (liste, siteli, sunucu) =
            sitesiz_isletmeler(&ajan, "İstanbul", "Beşiktaş", "Restoran", &konum)
                .expect("tarama çalışmalı");
        println!("yanıtı veren sunucu: {sunucu}");
        println!(
            "web sitesi olmayan: {}, web sitesi olan: {}",
            liste.len(),
            siteli
        );
        let koordinatli = liste
            .iter()
            .filter(|isletme| isletme.konum.is_some())
            .count();
        let saatli = liste
            .iter()
            .filter(|isletme| !isletme.calisma_saatleri.is_empty())
            .count();
        println!("koordinatlı: {koordinatli}, çalışma saati olan: {saatli}");
        assert!(!liste.is_empty(), "gerçek işletme bulunmalı");
        assert!(koordinatli > 0, "en az bir kayıtta konum olmalı");
        assert!(
            liste.len() <= SONUC_SINIRI,
            "sorgu başına en fazla {SONUC_SINIRI} kayıt gelmeli"
        );
        assert!(liste.iter().all(|isletme| !isletme.name.is_empty()
            && isletme.kaynak.starts_with("https://www.openstreetmap.org/")));
    }

    #[test]
    fn tum_iller_ve_ilceler_listelenir() {
        assert_eq!(iller::ILLER.len(), 81, "81 il olmalı");
        assert_eq!(iller::ILCELER, 973, "973 ilçe olmalı");
        assert_eq!(iller::ILLER[0].0, "İstanbul", "varsayılan il en üstte");
        assert!(iller::ILLER[0].1.contains(&"Beşiktaş"));
        assert_eq!(iller::ILLER[0].1.len(), 39, "İstanbul'un 39 ilçesi var");

        let toplam: usize = iller::ILLER.iter().map(|(_, ilceler)| ilceler.len()).sum();
        assert_eq!(toplam, iller::ILCELER);

        let mut isimler: Vec<&str> = iller::ILLER.iter().map(|(il, _)| *il).collect();
        isimler.sort();
        isimler.dedup();
        assert_eq!(isimler.len(), 81, "il adları benzersiz olmalı");

        for (il, ilceler) in iller::ILLER {
            assert!(!ilceler.is_empty(), "{il} ilçesiz kalmamalı");
        }
    }

    #[test]
    fn arama_turkce_karakterleri_sadelestirir() {
        assert_eq!(arama_anahtari("İstanbul"), "istanbul");
        assert_eq!(arama_anahtari("Iğdır"), "igdir");
        assert_eq!(arama_anahtari("Güzellik Salonu"), "guzellik salonu");
        assert!(arama_anahtari("Çankaya").contains(&arama_anahtari("cank")));
    }

    #[test]
    fn her_sektor_tek_adli_ve_osm_eslemeli() {
        // Sektör adları tek başına olmalı: "Tekstil / Giyim" gibi bileşik ad yok.
        for tanim in SEKTORLER {
            assert!(
                !tanim.ad.contains('/'),
                "bileşik sektör adı kalmamalı: {}",
                tanim.ad
            );
        }
        for tanim in SEKTORLER {
            assert!(!tanim.osm.is_empty(), "OSM eşlemesi yok: {}", tanim.ad);
            assert!(
                !osm_kurallari(tanim.ad).is_empty(),
                "eşleme yok: {}",
                tanim.ad
            );
            assert!(
                !demo_bolumu(tanim.ad).is_empty(),
                "demo bölümü yok: {}",
                tanim.ad
            );
        }

        // Adlar benzersiz olmalı; aynı sektör iki kez listelenmemeli.
        let mut adlar: Vec<&str> = SEKTORLER.iter().map(|tanim| tanim.ad).collect();
        let toplam = adlar.len();
        adlar.sort_unstable();
        adlar.dedup();
        assert_eq!(adlar.len(), toplam, "sektör adları benzersiz olmalı");

        // Tek başına sektörler: "Tekstil" ve "Giyim" ayrı ayrı bulunmalı.
        assert!(SEKTORLER.iter().any(|tanim| tanim.ad == "Giyim"));
        assert!(SEKTORLER.iter().any(|tanim| tanim.ad == "Tekstil"));
        assert!(osm_kurallari("Giyim")[0].1.contains("clothes"));
        assert!(
            SEKTORLER.len() >= 200,
            "sektör listesi kapsamlı olmalı: {}",
            SEKTORLER.len()
        );

        // Her sektörün grup başlığı var ve her grup tek blok halinde sıralı.
        let mut gruplar: Vec<&str> = Vec::new();
        for tanim in SEKTORLER {
            assert!(!tanim.grup.is_empty(), "grup başlığı yok: {}", tanim.ad);
            if gruplar.last() != Some(&tanim.grup) {
                assert!(
                    !gruplar.contains(&tanim.grup),
                    "grup bölünmüş: {}",
                    tanim.grup
                );
                gruplar.push(tanim.grup);
            }
        }
        assert!(gruplar.len() >= 34, "grup sayısı: {}", gruplar.len());
        assert_eq!(sektor_grubu("Giyim"), "Giyim & Aksesuar");
        assert_eq!(sektor_grubu("Spor Salonu"), "Spor & Fitness");
        assert_eq!(sektor_grubu("Düğün Salonu"), "Mekân & Etkinlik");
        assert_eq!(sektor_grubu("Halı"), "Tekstil & Ev Tekstili");
        assert_eq!(sektor_grubu("Diş Kliniği"), "Sağlık Kuruluşları");
        assert_eq!(sektor_grubu("Tesisatçı"), "Tesisat & Güvenlik");
        assert_eq!(sektor_grubu("Nakliyat"), "Lojistik & Seyahat");
        assert_eq!(sektor_grubu("Peyzaj"), "Çevre & Bakım");
        assert_eq!(sektor_grubu("Noter"), "Hukuk & Mali");
        assert_eq!(sektor_grubu("Mobilya"), "Mobilya & Dekorasyon");
        assert_eq!(sektor_grubu("Elektronik"), "Elektronik & Teknoloji");
        assert_eq!(sektor_grubu("Pet Kuaför"), "Evcil Hayvan");
        assert_eq!(sektor_grubu("Fabrika"), "Sanayi & Üretim");
        assert_eq!(sektor_grubu("Arıcılık"), "Tarım & Hayvancılık");
        assert_eq!(sektor_grubu("Vergi Danışmanı"), "Hukuk & Mali");
        assert_eq!(sektor_grubu("Bilinmeyen Sektör"), "Diğer");

        // Mali müşavir ile vergi danışmanı ayrı kurallar kullanmalı; aynı işletmeler
        // iki sektörde birden listelenmesin.
        assert_eq!(osm_kurallari("Muhasebe"), vec![("office", "^accountant$")]);
        assert_eq!(
            osm_kurallari("Vergi Danışmanı"),
            vec![("office", "^tax_advisor$")]
        );

        // Gruplar tarama için küçük kalmalı: en kalabalık grup en fazla 15 sektör.
        let en_kalabalik = gruplar
            .iter()
            .map(|grup| SEKTORLER.iter().filter(|tanim| tanim.grup == *grup).count())
            .max()
            .unwrap_or(0);
        assert!(
            en_kalabalik <= 15,
            "en kalabalık grup {en_kalabalik} sektör"
        );
    }

    /// Google Business Profile / Places kategorilerinden eklenen sektörler
    /// katalogda kalmalı ve OSM eşlemesi gerçek bir etikete dayanmalı.
    #[test]
    fn google_kategorilerinden_eklenen_sektorler_katalogda() {
        for (ad, anahtar, desen) in [
            ("Berber", "shop", "hairdresser"),
            ("Tırnak Stüdyosu", "shop", "nails"),
            ("Solaryum", "leisure", "tanning_salon"),
            ("Oto Kaporta", "shop", "car_repair"),
            ("Oto Muayene", "amenity", "vehicle_inspection"),
            ("Oto Döşeme", "craft", "upholsterer"),
            ("Diş Laboratuvarı", "craft", "dental_technician"),
            ("Göz Kliniği", "healthcare", "optometrist"),
            ("Yoga Stüdyosu", "sport", "yoga"),
            ("Pilates Stüdyosu", "sport", "pilates"),
            ("Tercüme Bürosu", "office", "translator"),
            ("Grafik Tasarım", "office", "graphic_design"),
            ("Ortak Çalışma Ofisi", "office", "coworking"),
            ("Motel", "tourism", "motel"),
            ("Nargile Kafe", "amenity", "hookah_lounge"),
            ("Hediyelik Eşya", "shop", "gift"),
            ("Elektronik Tamiri", "craft", "electronics_repair"),
            ("Pet Otel", "amenity", "animal_boarding"),
            ("Kebapçı", "amenity", "restaurant"),
            ("Dönerci", "amenity", "fast_food"),
            ("Kahveci", "amenity", "cafe"),
            ("Simitçi", "shop", "bakery"),
            ("Yemek Servisi", "craft", "catering"),
            ("Gelinlik", "shop", "clothes"),
            ("Perde & Stor", "shop", "window_blind"),
            ("Güvenlik Kamerası", "shop", "security"),
            ("Butik Otel", "tourism", "hotel"),
            ("Karavan Parkı", "tourism", "caravan_site"),
            ("Öğrenci Yurdu", "amenity", "dormitory"),
            ("Meslek Kursu", "amenity", "training"),
            ("Web Tasarım", "office", "it"),
            ("Kurye", "shop", "courier"),
            ("Halı Saha", "leisure", "pitch"),
            ("Karaoke", "amenity", "karaoke_box"),
            ("Mini Golf", "leisure", "miniature_golf"),
            ("Planetaryum", "amenity", "planetarium"),
            ("Temizlik Şirketi", "shop", "cleaning"),
            ("Dernek", "office", "association"),
            ("Zirai İlaç", "shop", "agrarian"),
        ] {
            let kurallar = osm_kurallari(ad);
            assert!(!kurallar.is_empty(), "{ad} katalogda yok");
            assert!(
                kurallar
                    .iter()
                    .any(|(kural_anahtari, kural_deseni)| *kural_anahtari == anahtar
                        && kural_deseni.contains(desen)),
                "{ad} eşlemesi: {kurallar:?}"
            );
            assert!(sektor_grubu(ad) != "Diğer", "{ad} grubu tanımsız");
            assert!(!demo_bolumu(ad).is_empty(), "{ad} demo şablonu yok");
        }
        assert!(
            SEKTORLER.len() >= 315,
            "katalog daralmamalı: {}",
            SEKTORLER.len()
        );
    }

    #[test]
    fn sektor_panelinde_gruplar_ve_sektorler_tutarli() {
        let gruplar = sektor_gruplari();
        assert_eq!(gruplar.len(), 47, "grup sayısı: {}", gruplar.len());

        // Gruplar tekrarsız ve katalog sırasıyla gelmeli.
        let mut sirali = gruplar.clone();
        sirali.dedup();
        assert_eq!(sirali.len(), gruplar.len(), "grup tekrarı olmamalı");

        // Her grubun sektörleri var ve toplam, kataloğun tamamına eşit.
        let toplam: usize = gruplar.iter().map(|grup| grup_sektorleri(grup).len()).sum();
        assert_eq!(toplam, SEKTORLER.len());
        assert!(!grup_sektorleri("Kuyum & Saat").is_empty());
        assert_eq!(
            grup_sektorleri("Kuyum & Saat"),
            vec!["Kuyumcu", "Saatçi", "Saat Tamiri", "Gümüş"]
        );

        // Panelden gelen sıra numarası katalogdaki sektöre işaret etmeli.
        for sira in [0usize, 1, 5, 40, 120, SEKTORLER.len() - 1] {
            let ad = SEKTORLER[sira].ad;
            assert_eq!(SEKTORLER[sektor_sirasi(ad)].ad, ad);
        }
    }

    #[test]
    fn google_kotasi_gunluk_ve_aylik_siniri_korur() {
        let mut kota = GoogleKota::default();
        for _ in 0..3 {
            assert_eq!(
                kota_karari(&mut kota, "2026-11-01", "2026-11", 3, 10),
                KotaKarari::Izin
            );
        }
        assert_eq!(
            kota_karari(&mut kota, "2026-11-01", "2026-11", 3, 10),
            KotaKarari::GunlukDolu
        );
        // Ertesi gün günlük sayaç sıfırlanır, aylık devam eder.
        assert_eq!(
            kota_karari(&mut kota, "2026-11-02", "2026-11", 3, 4),
            KotaKarari::Izin
        );
        assert_eq!(
            kota_karari(&mut kota, "2026-11-02", "2026-11", 3, 4),
            KotaKarari::AylikDolu
        );
        // Yeni ay her iki sayacı da sıfırlar.
        assert_eq!(
            kota_karari(&mut kota, "2026-12-01", "2026-12", 3, 4),
            KotaKarari::Izin
        );
        assert_eq!(kota.gunluk, 1);
        assert_eq!(kota.aylik, 1);
    }

    #[test]
    fn google_saatleri_osm_bicimine_cevrilir() {
        let gunler: Vec<String> = [
            "Monday: 9:00 AM – 6:00 PM",
            "Tuesday: 9:00 AM – 6:00 PM",
            "Wednesday: 9:00 AM – 6:00 PM",
            "Thursday: 9:00 AM – 6:00 PM",
            "Friday: 9:00 AM – 6:00 PM",
            "Saturday: 10:00 AM – 2:00 PM",
            "Sunday: Closed",
        ]
        .iter()
        .map(|satir| satir.to_string())
        .collect();
        assert_eq!(
            google_saatleri_osm(&gunler),
            "Mo-Fr 09:00-18:00; Sa 10:00-14:00; Su off"
        );
        // Saatleri aynı olan hafta tek aralığa toplanır.
        let yedi_gun: Vec<String> = (0..7)
            .map(|_| "Monday: 12:00 PM – 12:00 AM".to_string())
            .collect();
        assert_eq!(google_saatleri_osm(&yedi_gun), "Mo-Su 12:00-24:00");
        // Tanınmayan biçim saat uydurmaz.
        assert_eq!(google_saatleri_osm(&["Monday: ?".to_string()]), "");
    }

    #[test]
    fn google_yanitindan_siteli_kayitlar_ayiklanir() {
        let ham = r#"{"places":[
            {"id":"abc","displayName":{"text":"Moda Kuaför"},
             "formattedAddress":"Moda Cd. 1, Kadıköy","location":{"latitude":40.98,"longitude":29.02},
             "nationalPhoneNumber":"0216 000 00 00",
             "regularOpeningHours":{"weekdayDescriptions":[
                "Monday: 9:00 AM – 6:00 PM","Tuesday: 9:00 AM – 6:00 PM",
                "Wednesday: 9:00 AM – 6:00 PM","Thursday: 9:00 AM – 6:00 PM",
                "Friday: 9:00 AM – 6:00 PM","Saturday: 10:00 AM – 2:00 PM","Sunday: Closed"]}},
            {"id":"def","displayName":{"text":"Siteli Kuaför"},"websiteUri":"https://ornek.com"},
            {"id":"ghi","displayName":{"text":"Gece Kuaförü"},"formattedAddress":"X"}
        ]}"#;
        let (liste, siteli) = google_isle(ham, "Kuaför").expect("yanıt çözümlenmeli");
        assert_eq!(siteli, 1, "web siteli kayıt ayıklandı");
        assert_eq!(liste.len(), 2);
        let moda = liste
            .iter()
            .find(|lead| lead.name == "Moda Kuaför")
            .unwrap();
        assert_eq!(moda.phone, "0216 000 00 00");
        assert_eq!(moda.konum, Some((40.98, 29.02)));
        assert_eq!(
            moda.calisma_saatleri,
            "Mo-Fr 09:00-18:00; Sa 10:00-14:00; Su off"
        );
        // Saklanabilen tek Google alanı place ID; kaynak bağlantısı onu taşır.
        assert!(moda.kaynak.contains("place_id:abc"));
    }

    #[test]
    fn ilerleme_orani_sinirlari_korur() {
        assert_eq!(ilerleme_orani(0, 0), 0.0);
        assert!((ilerleme_orani(13, 39) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(ilerleme_orani(39, 39), 1.0);
        // Tamamlanan sayı toplamı aşsa bile çubuk taşmamalı.
        assert_eq!(ilerleme_orani(50, 39), 1.0);
    }

    #[test]
    fn bos_ilce_taramasi_il_geneline_gecer() {
        assert!(
            il_geneline_gecilir(0, false),
            "boş ilçe il geneline geçmeli"
        );
        // İl geneli de boşsa tarama yeniden başlatılmaz.
        assert!(!il_geneline_gecilir(0, true));
        assert!(!il_geneline_gecilir(3, false));
    }

    #[test]
    fn il_geneli_kota_ilcelere_esit_dagilir() {
        // 20 kayıt hakkı, 39 ilçe: her ilçeye en az 1 hak düşer.
        assert_eq!(ilce_kotasi(20, 39), 1);
        // Son ilçe kalınca kalan tüm hak oraya gider.
        assert_eq!(ilce_kotasi(20, 1), 20);
        assert_eq!(ilce_kotasi(5, 3), 2);
        // Kayıt hakkı veya ilçe kalmadıysa tarama sürmez.
        assert_eq!(ilce_kotasi(0, 7), 0);
        assert_eq!(ilce_kotasi(7, 0), 0);

        // Kota, toplam hakkı hiçbir zaman aşmaz.
        let mut kalan = SONUC_SINIRI;
        for kalan_ilce in (1..=39).rev() {
            let kota = ilce_kotasi(kalan, kalan_ilce);
            assert!(kota <= kalan, "kota kalan hakkı aşmamalı");
            kalan -= kota.min(kalan);
        }
        assert_eq!(kalan, 0, "tüm ilçeler gezilince hak tükenmeli");
    }
}
