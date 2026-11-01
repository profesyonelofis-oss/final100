#![windows_subsystem = "windows"]

use chrono::{Datelike, Timelike};
use eframe::egui;
use std::collections::HashSet;
use std::fs;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

mod iller;

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
}

/// Arka plandaki tarama/gönderim işinin arayüze gönderdiği mesajlar.
enum Mesaj {
    Log(String),
    /// İl geneli taramasında tamamlanan ilçe sayısı ve toplam ilçe.
    Ilerleme { tamamlanan: usize, toplam: usize },
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
    
    whatsapp_phone: String,
    whatsapp_status: String,
    is_whatsapp_connected: bool,
    whatsapp_acik: bool,
    detay_idx: Option<usize>,
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

    let kenarli = |w: &mut egui::style::WidgetVisuals, dolgu: egui::Color32, kenar: egui::Color32| {
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
        (
            egui::TextStyle::Heading,
            baslik_font(18.0),
        ),
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
    let boy = boy.unwrap_or_else(|| {
        egui::vec2(
            metin_genisligi(ui, metin, 12.0) + 32.0,
            30.0,
        )
    });
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
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, metin, baslik_font(12.0), yazi);
    yanit
}

/// Çerçeveli ikincil buton.
fn ikincil_buton(
    ui: &mut egui::Ui,
    metin: &str,
    etkin: bool,
    boy: Option<egui::Vec2>,
) -> egui::Response {
    let boy = boy.unwrap_or_else(|| {
        egui::vec2(
            metin_genisligi(ui, metin, 11.5) + 28.0,
            30.0,
        )
    });
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
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, metin, govde_font(11.5), yazi);
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
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(7.0), KENAR.gamma_multiply(t * 0.8));
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
        ui.painter()
            .add(egui::Shape::line(onay, egui::Stroke::new(1.7_f32, KOYU_YAZI)));
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
        }
    }
}

/// Hazır mesajla WhatsApp sohbetini açar; göndermeyi kullanıcı yapar.
fn whatsapp_ac(lead: &mut BusinessLead, gonderen: &str, logs: &mut Vec<String>) {
    let Some(numara) = whatsapp_numarasi(&lead.phone) else {
        return;
    };
    if lead.demo_yolu.is_none() {
        match demo_uret(lead, gonderen) {
            Ok(yol) => lead.demo_yolu = Some(yol),
            Err(hata) => logs.push(format!("[!] {hata}")),
        }
    }
    let url = format!(
        "https://wa.me/{numara}?text={}",
        url_encode(&whatsapp_mesaji(lead))
    );
    match tarayicida_ac(&url) {
        Ok(()) => {
            lead.mesaj_hazir = true;
            logs.push(format!(
                "[✓] {} için WhatsApp sohbeti hazırlandı; mesajı siz gönderiyorsunuz.",
                lead.name
            ));
        }
        Err(hata) => logs.push(format!("[!] WhatsApp açılamadı: {hata}")),
    }
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
) -> bool {
    let secili = lead.selected;
    let kategori_rengi = sektor_rengi(&lead.category);
    let mut detay_iste = false;

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
                        });
                        ui.label(
                            egui::RichText::new(&lead.address)
                                .size(11.5)
                                .color(SOLUK),
                        );

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
                                alt_satir
                                    .push_str(&calisma_saatleri_metni(&lead.calisma_saatleri));
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
                            let wa = birincil_buton(ui, wa_metin, numara.is_some(), None);
                            let wa = if numara.is_none() {
                                wa.on_hover_text("Bu işletmenin telefon numarası kayıtta yok")
                            } else {
                                wa
                            };
                            if wa.clicked() {
                                whatsapp_ac(lead, gonderen, logs);
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
        ui.make_persistent_id(("kart", &lead.kaynak, &lead.name)).with("hover"),
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

    detay_iste
}

impl LeadApp {
    /// Seçilen işletmenin ayrıntılarını ve aksiyonlarını sağ panelde gösterir.
    fn detay_paneli(&mut self, ctx: &egui::Context, gonderen: &str) {
        let Some(idx) = self.detay_idx else {
            return;
        };
        if idx >= self.leads.len() {
            self.detay_idx = None;
            return;
        }

        let mut kapat = false;
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

                let aksiyon_yuksekligi = 148.0;
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
                    whatsapp_ac(lead, gonderen, &mut self.logs);
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
                ui.add_space(2.0);
                ui.hyperlink_to(
                    egui::RichText::new(kaynak_etiketi(&lead.kaynak))
                        .size(10.5)
                        .color(DIM),
                    &lead.kaynak,
                );
            });

        if kapat {
            self.detay_idx = None;
        }
    }

    /// Seçili il, ilçe (ya da il geneli) ve sektörle yeni bir tarama başlatır.
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
        thread::spawn(move || tarama_calistir(tx, il, ilce, sektor));
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
                                    if grup_satiri(&mut kolonlar[idx % 2], grup, sayi, *grup == aktif)
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
                                        egui::RichText::new(format!("{} sektör", sektorler.len()))
                                            .size(10.0)
                                            .color(DIM),
                                    );
                                },
                            );
                        });
                        ui.add_space(8.0);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for ad in sektorler {
                            let secili = self.sectors[self.selected_sector_idx] == ad;
                            if sektor_satiri(ui, ad, "", secili).clicked() {
                                secilen_sektor = Some(sektor_sirasi(ad));
                            }
                        }
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
                                        egui::RichText::new(format!("{} sonuç", bulunanlar.len()))
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
                                    let secili = self.sectors[self.selected_sector_idx] == *ad;
                                    if sektor_satiri(ui, ad, sektor_grubu(ad), secili).clicked() {
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
                        ui.painter()
                            .circle_filled(nokta.center(), 3.5, if bagli { YESIL } else { DIM });
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
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if hayalet_buton(ui, "Düzenle").clicked() {
                                    self.whatsapp_acik = true;
                                }
                            },
                        );
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
                Mesaj::TaramaBitti(bulunanlar) => {
                    self.is_running = false;
                    self.ilerleme = None;
                    self.detay_idx = None;
                    let adet = bulunanlar.len();
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
                        self.logs.push(format!("[✓] {adet} işletme listeye eklendi."));
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
        let demo_hazir = self.leads.iter().filter(|lead| lead.demo_yolu.is_some()).count();
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
                    let (cizgi, _) = ui.allocate_exact_size(egui::vec2(1.0, 30.0), egui::Sense::hover());
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

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
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
                            let (nokta, _) = ui.allocate_exact_size(
                                egui::vec2(7.0, 7.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(nokta.center(), 3.0, YESIL);
                        },
                    );
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
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if hayalet_buton(ui, "Sıfırla").clicked() {
                                self.selected_sector_idx = 0;
                                self.selected_district_idx = 0;
                                self.il_geneli = false;
                                self.bos_ilce_uyari = None;
                                self.city_search.clear();
                                self.district_search.clear();
                                self.sector_search.clear();
                            }
                        },
                    );
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
                let kaydirma_yuksekligi =
                    (ui.available_height() - wa_yuksekligi - 78.0).max(160.0);
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
                                    if !sorgu.is_empty()
                                        && !arama_anahtari(ad).contains(&sorgu)
                                    {
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
                        if self.selected_district_idx >= self.cities[self.selected_city_idx].1.len() {
                            self.selected_district_idx = 0;
                        }
                        let ilce = self.cities[self.selected_city_idx].1[self.selected_district_idx];
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
                                    if liste_satiri(ui, !il_geneli && secili_idx == idx, ad).clicked() {
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
            .resizable(true)                .default_height(100.0)
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
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if hayalet_buton(ui, "Temizle").clicked() {
                                self.logs.clear();
                            }
                        },
                    );
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
                ui.horizontal(|ui| {
                    let arama_alani = 250.0_f32.min(ui.available_width() * 0.4);
                    ui.allocate_ui_with_layout(
                        egui::vec2(arama_alani, 34.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            arama_kutusu(ui, &mut self.sonuc_arama, "Sonuçlarda ara");
                        },
                    );

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let demo_metin = if secili > 0 {
                                format!("Seçilenlere demo üret ({secili})")
                            } else {
                                "Seçilenlere demo üret".to_string()
                            };
                            let demo_butonu =
                                ikincil_buton(ui, &demo_metin, secili > 0, Some(egui::vec2(200.0, 32.0)));
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
                                    format!("[✓] {uretilen} demo hazırlandı, {hatali} kayıt üretilemedi.")
                                });
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
                        },
                    );
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
                {
                    let arama = arama_anahtari(&self.sonuc_arama);
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
                                let uyari =
                                    bos_ilce.as_deref().map(|ilce| (ilce, sektor));
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
                                    let metin = format!(
                                        "{} {} {}",
                                        lead.name, lead.address, lead.category
                                    );
                                    if !arama_anahtari(&metin).contains(&arama) {
                                        continue;
                                    }
                                }
                                gosterilen += 1;
                                if kart_ciz(ui, lead, &gonderen, logs, acik_detay == Some(idx)) {
                                    yeni_detay = if acik_detay == Some(idx) {
                                        None
                                    } else {
                                        Some(idx)
                                    };
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
const OVERPASS_URL: &str = "https://overpass-api.de/api/interpreter";

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

fn http_ajani() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent("NexusEnterprise/0.1 (Windows masaustu isletme kesif uygulamasi)")
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .build()
}

fn http_hata(hata: ureq::Error) -> String {
    match hata {
        ureq::Error::Status(kod, cevap) => {
            let govde = cevap.into_string().unwrap_or_default();
            let ozet: String = govde.chars().take(160).collect();
            format!("sunucu HTTP {kod} döndü: {ozet}")
        }
        ureq::Error::Transport(tasima) => format!("bağlantı kurulamadı: {tasima}"),
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
        Self {
            anahtar,
            gunluk_limit,
            aylik_limit,
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
    let deger: serde_json::Value = serde_json::from_str(ham)
        .map_err(|hata| format!("Google yanıtı çözümlenemedi: {hata}"))?;
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
            if !yer
                .get("websiteUri")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                siteli += 1;
                continue;
            }
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
                demo_yolu: None,
                mesaj_hazir: false,
            });
        }
    }

    isletmeler.sort_by_key(|a| a.name.to_lowercase());
    Ok((isletmeler, siteli))
}

/// Google Places Text Search isteği; alan maskesi Enterprise kademesine düşer
/// (websiteUri + çalışma saatleri), bu yüzden yalnızca bütçe varsa çağrılır.
fn google_arama(
    ajan: &ureq::Agent,
    anahtar: &str,
    il: &str,
    ilce: &str,
    sektor: &str,
) -> Result<(Vec<BusinessLead>, usize), String> {
    let govde = serde_json::json!({
        "textQuery": format!("{sektor} {ilce} {il}"),
        "languageCode": "tr",
        "regionCode": "TR",
        "maxResultCount": SONUC_SINIRI,
    });
    let cevap = ajan
        .post(GOOGLE_TEXT_SEARCH)
        .set("Content-Type", "application/json")
        .set("X-Goog-Api-Key", anahtar)
        .set(
            "X-Goog-FieldMask",
            "places.id,places.displayName,places.formattedAddress,places.location,places.websiteUri,places.nationalPhoneNumber,places.regularOpeningHours.weekdayDescriptions",
        )
        .send_string(&govde.to_string())
        .map_err(http_hata)?
        .into_string()
        .map_err(|hata| format!("Google yanıtı okunamadı: {hata}"))?;
    google_isle(&cevap, sektor)
}

/// OSM boş döndüğünde Google yedeğini dener. Anahtar yoksa veya günlük/aylık
/// bütçe dolduysa `None` döner ve günlüğe neden atlandığı yazılır.
#[allow(clippy::too_many_arguments)]
fn google_yedek(
    tx: &Sender<Mesaj>,
    ajan: &ureq::Agent,
    ayar: &GoogleAyar,
    kota: &mut GoogleKota,
    il: &str,
    ilce: &str,
    sektor: &str,
) -> Option<Vec<BusinessLead>> {
    if !ayar.acik() {
        return None;
    }
    match kota_karari(
        kota,
        &bugun_anahtari(),
        &ay_anahtari(),
        ayar.gunluk_limit,
        ayar.aylik_limit,
    ) {
        KotaKarari::Izin => {}
        KotaKarari::GunlukDolu => {
            let _ = tx.send(Mesaj::Log(format!(
                "[!] Google yedeği atlandı: günlük istek bütçesi dolu ({}/{}).",
                kota.gunluk, ayar.gunluk_limit
            )));
            return None;
        }
        KotaKarari::AylikDolu => {
            let _ = tx.send(Mesaj::Log(format!(
                "[!] Google yedeği atlandı: aylık istek bütçesi dolu ({}/{}).",
                kota.aylik, ayar.aylik_limit
            )));
            return None;
        }
    }
    kota_yaz(kota);
    let _ = tx.send(Mesaj::Log(format!(
        "[>] {ilce} / {il}: OSM'de kayıt yok, Google Places yedeği deneniyor (bugün {}/{}).",
        kota.gunluk, ayar.gunluk_limit
    )));

    match google_arama(ajan, &ayar.anahtar, il, ilce, sektor) {
        Ok((liste, siteli)) => {
            let _ = tx.send(Mesaj::Log(if liste.is_empty() {
                format!("[i] Google yedeği de boş döndü ({siteli} web siteli kayıt ayıklandı).")
            } else {
                format!(
                    "[✓] Google yedeği: {} kayıt eklendi ({} web siteli kayıt ayıklandı).",
                    liste.len(),
                    siteli
                )
            }));
            Some(liste)
        }
        Err(hata) => {
            let _ = tx.send(Mesaj::Log(format!("[!] Google yedeği başarısız: {hata}")));
            None
        }
    }
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
    let govde = ajan
        .get(&url)
        .call()
        .map_err(http_hata)?
        .into_string()
        .map_err(|hata| format!("konum yanıtı okunamadı: {hata}"))?;
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

/// Türkiye'deki işletme sektörleri (NACE Rev.2.1 faaliyet kolları ve esnaf
/// meslek kolları sadeleştirilerek) — her satır tek bir sektördür, bileşik ad
/// yoktur. Aynı sıra arayüzdeki listeye yansır.
const SEKTORLER: &[Sektor] = &[
    // --- Yiyecek & İçecek ---
    Sektor { ad: "Restoran", grup: "Yiyecek & İçecek", demo: Demo::Menu, osm: &[("amenity", "^restaurant$")] },
    Sektor { ad: "Kafe", grup: "Yiyecek & İçecek", demo: Demo::Kafe, osm: &[("amenity", "^cafe$")] },
    Sektor { ad: "Fast Food", grup: "Yiyecek & İçecek", demo: Demo::Menu, osm: &[("amenity", "^fast_food$")] },
    Sektor { ad: "Fırın", grup: "Yiyecek & İçecek", demo: Demo::Unlu, osm: &[("shop", "^bakery$")] },
    Sektor { ad: "Pastane", grup: "Yiyecek & İçecek", demo: Demo::Unlu, osm: &[("shop", "^pastry$")] },
    Sektor { ad: "Tatlıcı", grup: "Yiyecek & İçecek", demo: Demo::Unlu, osm: &[("shop", "^confectionery$")] },
    Sektor { ad: "Dondurmacı", grup: "Yiyecek & İçecek", demo: Demo::Unlu, osm: &[("amenity", "^ice_cream$"), ("shop", "^ice_cream$")] },
    Sektor { ad: "Bar", grup: "Yiyecek & İçecek", demo: Demo::Menu, osm: &[("amenity", "^bar$")] },
    Sektor { ad: "Meyhane", grup: "Yiyecek & İçecek", demo: Demo::Menu, osm: &[("amenity", "^pub$")] },
    Sektor { ad: "Büfe", grup: "Yiyecek & İçecek", demo: Demo::Menu, osm: &[("shop", "^kiosk$")] },
    Sektor { ad: "Şarap Evi", grup: "Yiyecek & İçecek", demo: Demo::Urun, osm: &[("shop", "^wine$")] },
    // --- Gıda & Perakende ---
    Sektor { ad: "Market", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^supermarket$")] },
    Sektor { ad: "Bakkal", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^convenience$")] },
    Sektor { ad: "Kasap", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^butcher$")] },
    Sektor { ad: "Manav", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^greengrocer$")] },
    Sektor { ad: "Şarküteri", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^(deli|cheese|dairy)$")] },
    Sektor { ad: "Balıkçı", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^seafood$")] },
    Sektor { ad: "Peynirci", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^cheese$")] },
    Sektor { ad: "Aktar", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^herbalist$")] },
    Sektor { ad: "Toptancı", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^wholesale$")] },
    Sektor { ad: "Çay Dükkânı", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^tea$")] },
    Sektor { ad: "Çikolata", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^chocolate$")] },
    Sektor { ad: "Dondurulmuş Gıda", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^frozen_food$")] },
    Sektor { ad: "Takviye Gıda", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^nutrition_supplements$")] },
    Sektor { ad: "Kuruyemişçi", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^nuts$")] },
    Sektor { ad: "Su Bayii", grup: "Gıda & Perakende", demo: Demo::Urun, osm: &[("shop", "^water$")] },
    // --- Giyim & Aksesuar ---
    Sektor { ad: "Giyim", grup: "Giyim & Aksesuar", demo: Demo::Urun, osm: &[("shop", "^(clothes|boutique)$")] },
    Sektor { ad: "Ayakkabı", grup: "Giyim & Aksesuar", demo: Demo::Urun, osm: &[("shop", "^shoes$")] },
    Sektor { ad: "Çanta", grup: "Giyim & Aksesuar", demo: Demo::Urun, osm: &[("shop", "^bags$")] },
    Sektor { ad: "Deri", grup: "Giyim & Aksesuar", demo: Demo::Urun, osm: &[("shop", "^leather$")] },
    // --- Kuyum & Saat ---
    Sektor { ad: "Kuyumcu", grup: "Kuyum & Saat", demo: Demo::Urun, osm: &[("shop", "^jewelry$")] },
    Sektor { ad: "Saatçi", grup: "Kuyum & Saat", demo: Demo::Urun, osm: &[("shop", "^watches$")] },
    // --- Tekstil & Ev Tekstili ---
    Sektor { ad: "Tekstil", grup: "Tekstil & Ev Tekstili", demo: Demo::Urun, osm: &[("shop", "^fabric$")] },
    Sektor { ad: "Tuhafiye", grup: "Tekstil & Ev Tekstili", demo: Demo::Urun, osm: &[("shop", "^haberdashery$")] },
    Sektor { ad: "Perdeci", grup: "Tekstil & Ev Tekstili", demo: Demo::Urun, osm: &[("shop", "^curtain$")] },
    Sektor { ad: "Halı", grup: "Tekstil & Ev Tekstili", demo: Demo::Urun, osm: &[("shop", "^carpet$")] },
    // --- Dikiş & Bakım ---
    Sektor { ad: "Terzi", grup: "Dikiş & Bakım", demo: Demo::Servis, osm: &[("craft", "^tailor$"), ("shop", "^tailor$")] },
    Sektor { ad: "Kuru Temizleme", grup: "Dikiş & Bakım", demo: Demo::Servis, osm: &[("shop", "^(dry_cleaning|laundry)$")] },
    Sektor { ad: "Ayakkabı Tamiri", grup: "Dikiş & Bakım", demo: Demo::Servis, osm: &[("craft", "^shoemaker$")] },
    // --- Ev & Yaşam ---
    Sektor { ad: "Mobilya", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^furniture$")] },
    Sektor { ad: "Mutfak Mağazası", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^kitchen$")] },
    Sektor { ad: "Banyo", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^bathroom_furnishing$")] },
    Sektor { ad: "Yatak", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^bed$")] },
    Sektor { ad: "Aydınlatma", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^lighting$")] },
    Sektor { ad: "Boya Bayii", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^paint$")] },
    Sektor { ad: "Nalbur", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^(hardware|doityourself)$")] },
    Sektor { ad: "Züccaciye", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^houseware$")] },
    Sektor { ad: "Bahçe Merkezi", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^garden_centre$")] },
    Sektor { ad: "Çiçekçi", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^florist$")] },
    Sektor { ad: "İç Dekorasyon", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^interior_decoration$")] },
    Sektor { ad: "Çerçeve", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^frame$")] },
    Sektor { ad: "Seramik", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^tiles$")] },
    Sektor { ad: "Süpürge Mağazası", grup: "Ev & Yaşam", demo: Demo::Urun, osm: &[("shop", "^vacuum_cleaner$")] },
    // --- Elektronik & Teknoloji ---
    Sektor { ad: "Elektronik", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^electronics$")] },
    Sektor { ad: "Telefon", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^mobile_phone$")] },
    Sektor { ad: "Bilgisayar", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^computer$")] },
    Sektor { ad: "Beyaz Eşya", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^appliance$")] },
    Sektor { ad: "Kamera Mağazası", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^camera$")] },
    Sektor { ad: "Ses Sistemleri", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^hifi$")] },
    Sektor { ad: "Oyun Konsolu", grup: "Elektronik & Teknoloji", demo: Demo::Urun, osm: &[("shop", "^video_games$")] },
    // --- Hobi & Kültür ---
    Sektor { ad: "Kitapçı", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^books$")] },
    Sektor { ad: "Kırtasiye", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^stationery$")] },
    Sektor { ad: "Fotokopi", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^copyshop$")] },
    Sektor { ad: "Oyuncakçı", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^toys$")] },
    Sektor { ad: "Müzik Aletleri", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^musical_instrument$")] },
    Sektor { ad: "Spor Mağazası", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^sports$")] },
    Sektor { ad: "Bisikletçi", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^bicycle$")] },
    Sektor { ad: "Avcılık", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^hunting$")] },
    Sektor { ad: "Balık Malzemeleri", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^fishing$")] },
    Sektor { ad: "Antikacı", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^antiques$")] },
    Sektor { ad: "İkinci El", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^second_hand$")] },
    Sektor { ad: "Kamp Malzemeleri", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^outdoor$")] },
    Sektor { ad: "Plak & CD", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^music$")] },
    Sektor { ad: "Sanat Malzemeleri", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^art$")] },
    Sektor { ad: "Parti Malzemeleri", grup: "Hobi & Kültür", demo: Demo::Urun, osm: &[("shop", "^party$")] },
    // --- Otomotiv ---
    Sektor { ad: "Oto Galeri", grup: "Otomotiv", demo: Demo::Urun, osm: &[("shop", "^car$")] },
    Sektor { ad: "Oto Yedek Parça", grup: "Otomotiv", demo: Demo::Urun, osm: &[("shop", "^car_parts$")] },
    Sektor { ad: "Motosiklet", grup: "Otomotiv", demo: Demo::Urun, osm: &[("shop", "^motorcycle$")] },
    // --- Medikal & Optik ---
    Sektor { ad: "Optik", grup: "Medikal & Optik", demo: Demo::Urun, osm: &[("shop", "^optician$")] },
    Sektor { ad: "Medikal", grup: "Medikal & Optik", demo: Demo::Urun, osm: &[("shop", "^medical_supply$")] },
    Sektor { ad: "İşitme Cihazı", grup: "Medikal & Optik", demo: Demo::Urun, osm: &[("shop", "^hearing_aids$")] },
    Sektor { ad: "Odyoloji", grup: "Medikal & Optik", demo: Demo::Klinik, osm: &[("healthcare", "^audiologist$")] },
    // --- Evcil Hayvan ---
    Sektor { ad: "Pet Shop", grup: "Evcil Hayvan", demo: Demo::Urun, osm: &[("shop", "^pet$")] },
    Sektor { ad: "Pet Kuaför", grup: "Evcil Hayvan", demo: Demo::Servis, osm: &[("shop", "^pet_grooming$")] },
    // --- Yaşam & Diğer ---
    Sektor { ad: "Tütüncü", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^tobacco$")] },
    Sektor { ad: "İçkici", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^alcohol$")] },
    Sektor { ad: "Depolama", grup: "Yaşam & Diğer", demo: Demo::Servis, osm: &[("shop", "^storage_rental$")] },
    Sektor { ad: "Bebek Mağazası", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^baby_goods$")] },
    Sektor { ad: "Gazete Bayii", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^newsagent$")] },
    Sektor { ad: "Şans Oyunları", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^lottery$")] },
    Sektor { ad: "Video Kiralama", grup: "Yaşam & Diğer", demo: Demo::Urun, osm: &[("shop", "^video$")] },
    // --- Sağlık ---
    Sektor { ad: "Eczane", grup: "Sağlık", demo: Demo::Urun, osm: &[("amenity", "^pharmacy$")] },
    Sektor { ad: "Diş Kliniği", grup: "Sağlık", demo: Demo::Klinik, osm: &[("amenity", "^dentist$"), ("healthcare", "^dentist$")] },
    Sektor { ad: "Klinik", grup: "Sağlık", demo: Demo::Klinik, osm: &[("amenity", "^(clinic|doctors)$"), ("healthcare", "^clinic$")] },
    Sektor { ad: "Hastane", grup: "Sağlık", demo: Demo::Klinik, osm: &[("amenity", "^hospital$")] },
    Sektor { ad: "Veteriner", grup: "Sağlık", demo: Demo::Klinik, osm: &[("amenity", "^veterinary$")] },
    Sektor { ad: "Fizyoterapi", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^physiotherapist$")] },
    Sektor { ad: "Psikolog", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^psychotherapist$")] },
    Sektor { ad: "Laboratuvar", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^laboratory$")] },
    Sektor { ad: "Diyaliz Merkezi", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^dialysis$")] },
    Sektor { ad: "Rehabilitasyon", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^rehabilitation$")] },
    Sektor { ad: "Podolog", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^podiatrist$")] },
    Sektor { ad: "Konuşma Terapisi", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^speech_therapist$")] },
    Sektor { ad: "Alternatif Tıp", grup: "Sağlık", demo: Demo::Klinik, osm: &[("healthcare", "^alternative$")] },
    // --- Güzellik & Bakım ---
    Sektor { ad: "Kuaför", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("shop", "^hairdresser$")] },
    Sektor { ad: "Güzellik Salonu", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("shop", "^beauty$"), ("leisure", "^spa$")] },
    Sektor { ad: "Masaj", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("shop", "^massage$")] },
    Sektor { ad: "Dövme", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("shop", "^tattoo$")] },
    Sektor { ad: "Hamam", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("amenity", "^public_bath$")] },
    Sektor { ad: "Fotoğrafçı", grup: "Güzellik & Bakım", demo: Demo::Galeri, osm: &[("craft", "^photographer$"), ("shop", "^photo$")] },
    Sektor { ad: "Kozmetik", grup: "Güzellik & Bakım", demo: Demo::Urun, osm: &[("shop", "^cosmetics$")] },
    Sektor { ad: "Parfümeri", grup: "Güzellik & Bakım", demo: Demo::Urun, osm: &[("shop", "^perfumery$")] },
    // --- Konaklama ---
    Sektor { ad: "Otel", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^hotel$")] },
    Sektor { ad: "Pansiyon", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^guest_house$")] },
    Sektor { ad: "Hostel", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^hostel$")] },
    Sektor { ad: "Kamp Alanı", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^(camp_site|caravan_site)$")] },
    Sektor { ad: "Apart Otel", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^apartment$")] },
    Sektor { ad: "Bungalov", grup: "Konaklama", demo: Demo::Otel, osm: &[("tourism", "^chalet$")] },
    Sektor { ad: "Plaj Tesisi", grup: "Konaklama", demo: Demo::Otel, osm: &[("leisure", "^beach_resort$")] },
    // --- Eğitim ---
    Sektor { ad: "Kurs", grup: "Eğitim", demo: Demo::Egitim, osm: &[("office", "^educational_institution$")] },
    Sektor { ad: "Sürücü Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^driving_school$")] },
    Sektor { ad: "Dil Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^language_school$")] },
    Sektor { ad: "Müzik Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^music_school$")] },
    Sektor { ad: "Kreş", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^(kindergarten|childcare)$")] },
    Sektor { ad: "Etüt Merkezi", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^prep_school$")] },
    Sektor { ad: "Okul", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^school$")] },
    Sektor { ad: "Kolej", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^college$")] },
    Sektor { ad: "Üniversite", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^university$")] },
    Sektor { ad: "Dans Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("leisure", "^dance$")] },
    Sektor { ad: "Yüzme Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("leisure", "^swimming_pool$")] },
    Sektor { ad: "Sanat Kursu", grup: "Eğitim", demo: Demo::Egitim, osm: &[("amenity", "^arts_centre$")] },
    // --- Yapı & Emlak ---
    Sektor { ad: "Mimarlık", grup: "Yapı & Emlak", demo: Demo::Ofis, osm: &[("office", "^architect$")] },
    Sektor { ad: "Mühendislik", grup: "Yapı & Emlak", demo: Demo::Ofis, osm: &[("office", "^engineer$")] },
    Sektor { ad: "İnşaat", grup: "Yapı & Emlak", demo: Demo::Ofis, osm: &[("office", "^(construction_company|company)$"), ("craft", "^(builder|scaffolder)$")] },
    Sektor { ad: "Emlak", grup: "Yapı & Emlak", demo: Demo::Ofis, osm: &[("office", "^estate_agent$")] },
    Sektor { ad: "Harita Mühendisi", grup: "Yapı & Emlak", demo: Demo::Ofis, osm: &[("office", "^surveyor$")] },
    // --- Hukuk & Mali ---
    Sektor { ad: "Muhasebe", grup: "Hukuk & Mali", demo: Demo::Ofis, osm: &[("office", "^accountant$")] },
    Sektor { ad: "Avukat", grup: "Hukuk & Mali", demo: Demo::Ofis, osm: &[("office", "^lawyer$")] },
    Sektor { ad: "Noter", grup: "Hukuk & Mali", demo: Demo::Ofis, osm: &[("office", "^notary$")] },
    Sektor { ad: "Vergi Danışmanı", grup: "Hukuk & Mali", demo: Demo::Ofis, osm: &[("office", "^tax_advisor$")] },
    // --- Finans & Resmi İşlemler ---
    Sektor { ad: "Banka", grup: "Finans & Resmi İşlemler", demo: Demo::Ofis, osm: &[("amenity", "^bank$")] },
    Sektor { ad: "Döviz Bürosu", grup: "Finans & Resmi İşlemler", demo: Demo::Ofis, osm: &[("amenity", "^bureau_de_change$")] },
    Sektor { ad: "Sigorta", grup: "Finans & Resmi İşlemler", demo: Demo::Ofis, osm: &[("office", "^insurance$")] },
    Sektor { ad: "Postane", grup: "Finans & Resmi İşlemler", demo: Demo::Ofis, osm: &[("amenity", "^post_office$")] },
    Sektor { ad: "Finans Danışmanı", grup: "Finans & Resmi İşlemler", demo: Demo::Ofis, osm: &[("office", "^financial$")] },
    // --- Bilişim & Kurumsal Destek ---
    Sektor { ad: "Yazılım", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^it$")] },
    Sektor { ad: "Reklam", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^advertising_agency$")] },
    Sektor { ad: "Matbaa", grup: "Bilişim & Kurumsal Destek", demo: Demo::Servis, osm: &[("craft", "^printer$"), ("shop", "^copyshop$")] },
    Sektor { ad: "İnsan Kaynakları", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^employment_agency$")] },
    Sektor { ad: "Danışmanlık", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^consulting$")] },
    Sektor { ad: "Telekomünikasyon", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^telecommunication$")] },
    Sektor { ad: "Yayınevi", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^publisher$")] },
    Sektor { ad: "Gazete", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^newspaper$")] },
    Sektor { ad: "Güvenlik", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^security$")] },
    Sektor { ad: "Araştırma", grup: "Bilişim & Kurumsal Destek", demo: Demo::Ofis, osm: &[("office", "^research$")] },
    // --- Lojistik & Seyahat ---
    Sektor { ad: "Nakliyat", grup: "Lojistik & Seyahat", demo: Demo::Ofis, osm: &[("office", "^(logistics|moving_company)$")] },
    Sektor { ad: "Turizm Acentası", grup: "Lojistik & Seyahat", demo: Demo::Ofis, osm: &[("shop", "^travel_agency$"), ("office", "^travel_agent$")] },
    // --- Oto Servis ---
    Sektor { ad: "Oto Tamir", grup: "Oto Servis", demo: Demo::Servis, osm: &[("shop", "^car_repair$")] },
    Sektor { ad: "Lastikçi", grup: "Oto Servis", demo: Demo::Servis, osm: &[("shop", "^tyres$")] },
    Sektor { ad: "Oto Yıkama", grup: "Oto Servis", demo: Demo::Servis, osm: &[("amenity", "^car_wash$")] },
    Sektor { ad: "Oto Kiralama", grup: "Oto Servis", demo: Demo::Servis, osm: &[("amenity", "^car_rental$")] },
    Sektor { ad: "Akaryakıt", grup: "Oto Servis", demo: Demo::Servis, osm: &[("amenity", "^fuel$")] },
    Sektor { ad: "Motosiklet Tamiri", grup: "Oto Servis", demo: Demo::Servis, osm: &[("shop", "^motorcycle_repair$")] },
    Sektor { ad: "Şarj İstasyonu", grup: "Oto Servis", demo: Demo::Servis, osm: &[("amenity", "^charging_station$")] },
    // --- Tesisat & Güvenlik ---
    Sektor { ad: "Tesisatçı", grup: "Tesisat & Güvenlik", demo: Demo::Servis, osm: &[("craft", "^plumber$")] },
    Sektor { ad: "Elektrikçi", grup: "Tesisat & Güvenlik", demo: Demo::Servis, osm: &[("craft", "^electrician$")] },
    Sektor { ad: "Klima", grup: "Tesisat & Güvenlik", demo: Demo::Servis, osm: &[("craft", "^hvac$")] },
    Sektor { ad: "Çilingir", grup: "Tesisat & Güvenlik", demo: Demo::Servis, osm: &[("craft", "^(key_cutting|locksmith)$")] },
    // --- İnşaat Ustaları ---
    Sektor { ad: "Boyacı", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^painter$")] },
    Sektor { ad: "Marangoz", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^(carpenter|joiner)$")] },
    Sektor { ad: "Camcı", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^glaziery$")] },
    Sektor { ad: "Fayansçı", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^tiler$")] },
    Sektor { ad: "Sıvacı", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^plasterer$")] },
    Sektor { ad: "Çatıcı", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^roofer$")] },
    Sektor { ad: "Zemin Döşeme", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^floorer$")] },
    Sektor { ad: "Taş Ustası", grup: "İnşaat Ustaları", demo: Demo::Servis, osm: &[("craft", "^stonemason$")] },
    // --- Metal & Doğrama ---
    Sektor { ad: "Demirci", grup: "Metal & Doğrama", demo: Demo::Servis, osm: &[("craft", "^blacksmith$")] },
    Sektor { ad: "Kaynakçı", grup: "Metal & Doğrama", demo: Demo::Servis, osm: &[("craft", "^welder$")] },
    Sektor { ad: "PVC Doğrama", grup: "Metal & Doğrama", demo: Demo::Servis, osm: &[("craft", "^window_construction$")] },
    Sektor { ad: "Metal İşleri", grup: "Metal & Doğrama", demo: Demo::Servis, osm: &[("craft", "^metal_construction$")] },
    // --- Çevre & Bakım ---
    Sektor { ad: "Peyzaj", grup: "Çevre & Bakım", demo: Demo::Servis, osm: &[("craft", "^gardener$")] },
    Sektor { ad: "İlaçlama", grup: "Çevre & Bakım", demo: Demo::Servis, osm: &[("craft", "^pest_control$")] },
    // --- Spor & Fitness ---
    Sektor { ad: "Spor Salonu", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^(fitness_centre|sports_centre)$")] },
    Sektor { ad: "Sauna", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^sauna$")] },
    Sektor { ad: "Binicilik", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^horse_riding$")] },
    Sektor { ad: "Golf", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^golf_course$")] },
    Sektor { ad: "Buz Pisti", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^ice_rink$")] },
    Sektor { ad: "Su Parkı", grup: "Spor & Fitness", demo: Demo::Eglence, osm: &[("leisure", "^water_park$")] },
    // --- Mekân & Etkinlik ---
    Sektor { ad: "Düğün Salonu", grup: "Mekân & Etkinlik", demo: Demo::Eglence, osm: &[("amenity", "^(events_venue|community_centre)$")] },
    Sektor { ad: "Gece Kulübü", grup: "Mekân & Etkinlik", demo: Demo::Eglence, osm: &[("amenity", "^nightclub$")] },
    Sektor { ad: "Konser Salonu", grup: "Mekân & Etkinlik", demo: Demo::Eglence, osm: &[("amenity", "^music_venue$")] },
    Sektor { ad: "Konferans Merkezi", grup: "Mekân & Etkinlik", demo: Demo::Eglence, osm: &[("amenity", "^conference_centre$")] },
    // --- Sinema & Sahne ---
    Sektor { ad: "Sinema", grup: "Sinema & Sahne", demo: Demo::Eglence, osm: &[("amenity", "^cinema$")] },
    Sektor { ad: "Tiyatro", grup: "Sinema & Sahne", demo: Demo::Eglence, osm: &[("amenity", "^theatre$")] },
    // --- Oyun & Eğlence ---
    Sektor { ad: "Oyun Salonu", grup: "Oyun & Eğlence", demo: Demo::Eglence, osm: &[("leisure", "^amusement_arcade$")] },
    Sektor { ad: "Bowling", grup: "Oyun & Eğlence", demo: Demo::Eglence, osm: &[("leisure", "^bowling_alley$")] },
    Sektor { ad: "Lunapark", grup: "Oyun & Eğlence", demo: Demo::Eglence, osm: &[("tourism", "^theme_park$")] },
    Sektor { ad: "Oyun Parkı", grup: "Oyun & Eğlence", demo: Demo::Eglence, osm: &[("leisure", "^playground$")] },
    Sektor { ad: "Escape Oyunu", grup: "Oyun & Eğlence", demo: Demo::Eglence, osm: &[("leisure", "^escape_game$")] },
    Sektor { ad: "İnternet Kafe", grup: "Oyun & Eğlence", demo: Demo::Kafe, osm: &[("amenity", "^internet_cafe$")] },
    // --- Kültür & Gezi ---
    Sektor { ad: "Müze", grup: "Kültür & Gezi", demo: Demo::Eglence, osm: &[("tourism", "^museum$")] },
    Sektor { ad: "Sanat Galerisi", grup: "Kültür & Gezi", demo: Demo::Eglence, osm: &[("tourism", "^gallery$")] },
    Sektor { ad: "Hayvanat Bahçesi", grup: "Kültür & Gezi", demo: Demo::Eglence, osm: &[("tourism", "^zoo$")] },
    Sektor { ad: "Akvaryum", grup: "Kültür & Gezi", demo: Demo::Eglence, osm: &[("tourism", "^aquarium$")] },
    // --- Sanayi & Üretim ---
    Sektor { ad: "Fabrika", grup: "Sanayi & Üretim", demo: Demo::Urun, osm: &[("man_made", "^works$")] },
    Sektor { ad: "Şarap Üretimi", grup: "Sanayi & Üretim", demo: Demo::Urun, osm: &[("craft", "^winery$")] },
    Sektor { ad: "Bira Fabrikası", grup: "Sanayi & Üretim", demo: Demo::Urun, osm: &[("craft", "^brewery$")] },
    Sektor { ad: "Zeytinyağı", grup: "Sanayi & Üretim", demo: Demo::Urun, osm: &[("craft", "^oil_mill$")] },
    // --- Tarım & Hayvancılık ---
    Sektor { ad: "Tarım Makineleri", grup: "Tarım & Hayvancılık", demo: Demo::Urun, osm: &[("shop", "^agrarian$")] },
    Sektor { ad: "Arıcılık", grup: "Tarım & Hayvancılık", demo: Demo::Urun, osm: &[("craft", "^beekeeper$")] },
    Sektor { ad: "Çiftlik", grup: "Tarım & Hayvancılık", demo: Demo::Urun, osm: &[("landuse", "^farmyard$")] },
    Sektor { ad: "Sera", grup: "Tarım & Hayvancılık", demo: Demo::Urun, osm: &[("landuse", "^greenhouse_horticulture$")] },
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
        if WEB_SITESI_ETIKETLERI
            .iter()
            .any(|anahtar| etiket(tags, anahtar).is_some())
        {
            siteli += 1;
            continue;
        }

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

        let tur = eleman.get("type").and_then(|v| v.as_str()).unwrap_or("node");
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
            demo_yolu: None,
            mesaj_hazir: false,
        });
    }

    isletmeler.sort_by_key(|a| a.name.to_lowercase());
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
    govde.push_str("(\n");
    for (anahtar, desen) in kurallar {
        govde.push_str(&format!(
            "  nwr[\"{anahtar}\"~\"{desen}\"][!\"website\"][!\"contact:website\"][!\"url\"][!\"contact:url\"]{hedef};\n"
        ));
    }
    govde.push_str(&format!(");\nout center tags {SONUC_SINIRI};"));
    format!("[out:json][timeout:60];\n{govde}")
}

fn sitesiz_isletmeler(
    ajan: &ureq::Agent,
    il: &str,
    ilce: &str,
    sektor: &str,
    konum: &Konum,
) -> Result<(Vec<BusinessLead>, usize), String> {
    let kurallar = osm_kurallari(sektor);
    if kurallar.is_empty() {
        return Err(format!("'{sektor}' sektörü için OSM eşlemesi tanımlı değil"));
    }

    let istek = |sorgu: &str| -> Result<String, String> {
        ajan.post(OVERPASS_URL)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&format!("data={}", url_encode(sorgu)))
            .map_err(http_hata)?
            .into_string()
            .map_err(|hata| format!("tarama yanıtı okunamadı: {hata}"))
    };

    // 1) İlçenin gerçek idari sınır poligonu: komşu mahallelere taşmaz.
    if let Some(alan_id) = konum.alan_id {
        let sorgu = overpass_sorgusu(&kurallar, "(area.a)", Some(alan_id));
        let sonuc = overpass_isle(&istek(&sorgu)?, il, ilce, sektor)?;
        if !sonuc.0.is_empty() {
            return Ok(sonuc);
        }
    }

    // 2) Sınır poligonu yoksa veya boş döndüyse merkez yarıçapına düş.
    let hedef = format!(
        "(around:{},{:.7},{:.7})",
        konum.yaricap_m, konum.enlem, konum.boylam
    );
    let sorgu = overpass_sorgusu(&kurallar, &hedef, None);
    overpass_isle(&istek(&sorgu)?, il, ilce, sektor)
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
fn tarama_calistir(tx: Sender<Mesaj>, il: String, ilce: Option<String>, sektor: String) {
    match ilce {
        Some(ilce) => tarama_tek_ilce(&tx, &il, &ilce, &sektor),
        None => tarama_il_geneli(&tx, &il, &sektor),
    }
}

/// İlin tüm ilçelerini sırayla tarar; sonuçlar `SONUC_SINIRI` ile sınırlıdır.
fn tarama_il_geneli(tx: &Sender<Mesaj>, il: &str, sektor: &str) {
    let ilceler: Vec<&str> = iller::ILLER
        .iter()
        .find(|(ad, _)| *ad == il)
        .map(|(_, ilceler)| ilceler.to_vec())
        .unwrap_or_default();
    if ilceler.is_empty() {
        let _ = tx.send(Mesaj::Log(format!("[!] {il} için ilçe listesi bulunamadı.")));
        let _ = tx.send(Mesaj::TaramaBasarisiz);
        return;
    }

    let ajan = http_ajani();
    let ayar = GoogleAyar::oku();
    let mut google_kota = kota_oku();
    let mut google_denemesi = 0usize;
    let _ = tx.send(Mesaj::Log(format!(
        "[>] {il} il geneli: {} ilçe sırayla taranacak ({sektor}).",
        ilceler.len()
    )));

    let mut bulunanlar: Vec<BusinessLead> = Vec::new();
    let mut gorulen: HashSet<String> = HashSet::new();
    let mut siteli = 0usize;
    let mut atlanan = 0usize;
    let mut taranan = 0usize;

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

        let konum = match konum_bul(&ajan, il, ilce) {
            Ok(konum) => konum,
            Err(hata) => {
                atlanan += 1;
                let _ = tx.send(Mesaj::Log(format!("[!] {ilce} atlandı: {hata}")));
                thread::sleep(Duration::from_millis(1_100));
                continue;
            }
        };

        match sitesiz_isletmeler(&ajan, il, ilce, sektor, &konum) {
            Ok((mut liste, ilce_siteli)) => {
                // İlçe OSM'de boşsa ve tarama bütçesi varsa Google'a düşülür;
                // böylece tek tarama tüm ücretsiz kotayı tüketmez.
                if liste.is_empty() && google_denemesi < GOOGLE_TARAMA_SINIRI {
                    google_denemesi += 1;
                    if ayar.acik() {
                        if let Some(gelen) =
                            google_yedek(tx, &ajan, &ayar, &mut google_kota, il, ilce, sektor)
                        {
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
                let _ = tx.send(Mesaj::Log(format!("[!] {ilce} atlandı: {hata}")));
            }
        }

        thread::sleep(Duration::from_millis(1_100));
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
    let _ = tx.send(Mesaj::Log(format!(
        "[✓] {il} il geneli bitti: {} ilçe tarandı, {} kayıt, {} web siteli kayıt ayıklandı{}.",
        taranan,
        bulunanlar.len(),
        siteli,
        if atlanan == 0 {
            String::new()
        } else {
            format!(", {atlanan} ilçe atlandı")
        }
    )));
    let _ = tx.send(Mesaj::TaramaBitti(bulunanlar));
}

fn tarama_tek_ilce(tx: &Sender<Mesaj>, il: &str, ilce: &str, sektor: &str) {
    let ajan = http_ajani();
    let _ = tx.send(Mesaj::Log(format!("[>] {ilce} / {il} konumu aranıyor...")));
    let konum = match konum_bul(&ajan, il, ilce) {
        Ok(konum) => konum,
        Err(hata) => {
            let _ = tx.send(Mesaj::Log(format!("[!] Konum alınamadı: {hata}")));
            let _ = tx.send(Mesaj::TaramaBasarisiz);
            return;
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
    match sitesiz_isletmeler(&ajan, il, ilce, sektor, &konum) {
        Ok((mut liste, siteli)) => {
            // OSM bu ilçede boş döndü: bütçe varsa Google yedeği devreye girer.
            if liste.is_empty() {
                let ayar = GoogleAyar::oku();
                if ayar.acik() {
                    let mut kota = kota_oku();
                    if let Some(gelen) =
                        google_yedek(tx, &ajan, &ayar, &mut kota, il, ilce, sektor)
                    {
                        liste = gelen;
                    }
                } else {
                    let _ = tx.send(Mesaj::Log(
                        "[i] Google yedeği kapalı: GOOGLE_PLACES_API_KEY tanımlı değil, yalnızca OSM kullanılıyor."
                            .to_string(),
                    ));
                }
            }
            let ozet = if siteli == 0 {
                format!("[✓] {} web sitesi olmayan işletme bulundu.", liste.len())
            } else {
                format!(
                    "[✓] {} web sitesi olmayan işletme bulundu ({} web siteli kayıt ayıklandı).",
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
/// Overpass'tan istenen en fazla kayıt sayısı.
const SONUC_SINIRI: usize = 20;

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
    let temiz = cumle.trim_start_matches("İşletmeniz ").trim_end_matches('.');
    let mut karakterler = temiz.chars();
    match karakterler.next() {
        Some(ilk) => ilk.to_uppercase().collect::<String>() + karakterler.as_str(),
        None => String::new(),
    }
}

fn whatsapp_mesaji(isletme: &BusinessLead) -> String {
    let durum = whatsapp_durumu(isletme);
    whatsapp_mesaji_ile(isletme, durum.as_deref())
}

fn whatsapp_mesaji_ile(isletme: &BusinessLead, durum: Option<&str>) -> String {
    let ek = match durum {
        Some(durum) => format!(" {durum}"),
        None => String::new(),
    };
    let demo_notu = if durum.is_some() {
        "demoda harita, çalışma saatleri ve açık/kapalı göstergesi de var"
    } else {
        "demoda konum haritası ve işletme bilgileri hazır"
    };
    format!(
        "Merhaba, {} ({}) için hazırladığım web sitesi taslağını incelemek ister misiniz?{ek} 12 saat geçerli demo dosyasını bu sohbete ekliyorum; {demo_notu}. Beğenirseniz kendi alan adınızda yayına alabilirim.",
        isletme.name, isletme.address
    )
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
            ("☕", "Kahveler", "Filtre kahve, Türk kahvesi ve soğuk seçenekler."),
            ("🍰", "Tatlılar", "Günlük hazırlanan pasta ve tatlı çeşitleri."),
            ("🥐", "Atıştırmalıklar", "Sandviç, tost ve kahvaltı tabağı."),
        ],
    )
}

fn unlu_bolumu() -> String {
    hizmet_kartlari(
        "Ürünlerimiz",
        &[
            ("🥖", "Taze Üretim", "Her gün taze çıkan ekmek ve hamur işleri."),
            ("🎂", "Özel Sipariş", "Doğum günü ve davetler için özel pasta."),
            ("🤍", "Yanında", "Kahve, çay ve günlük atıştırmalıklar."),
        ],
    )
}

fn urun_bolumu() -> String {
    hizmet_kartlari(
        "Ürünler",
        &[
            ("🆕", "Yeni Gelenler", "Sezonun yeni ürünleri ve kampanyalar."),
            ("⭐", "Çok Satanlar", "Müşterilerin en çok tercih ettiği ürünler."),
            ("🚚", "Teslimat", "Mağazadan teslim veya adrese gönderim."),
        ],
    )
}

fn klinik_bolumu() -> String {
    hizmet_kartlari(
        "Hizmetlerimiz",
        &[
            ("🩺", "Muayene ve Kontrol", "Durum değerlendirmesi ve tedavi planı."),
            ("✨", "Tedavi", "Uzman kadroyla planlı ve takipli tedavi."),
            ("📅", "Randevu", "Uygun saate hızlı randevu."),
        ],
    )
}

fn otel_bolumu() -> String {
    hizmet_kartlari(
        "Odalar ve Konaklama",
        &[
            ("🛏️", "Standart Oda", "Konforlu yatak, klima ve ücretsiz Wi-Fi."),
            ("🌅", "Deluxe Oda", "Geniş oda, manzara ve kahvaltı dahil."),
            ("☕", "Kahvaltı & Olanaklar", "Açık büfe kahvaltı, otopark, resepsiyon."),
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
            ("📐", "Mimari Proje", "Konsept, ruhsat ve uygulama projeleri."),
            ("🛋️", "İç Mimarlık", "Mekân tasarımı ve malzeme seçimi."),
            ("🧱", "Tadilat & Uygulama", "Anahtar teslim tadilat ve denetim."),
        ],
        "Emlak" => &[
            ("🏠", "Satılık & Kiralık", "Bölgedeki güncel portföy."),
            ("📊", "Değerleme", "Mülkünüz için gerçekçi fiyat analizi."),
            ("📝", "Tapu & Sözleşme", "Süreç boyunca resmî işlem desteği."),
        ],
        "Muhasebe" => &[
            ("🧾", "Vergi & Beyanname", "Aylık ve yıllık beyanname takibi."),
            ("🏢", "Şirket Kuruluşu", "Kuruluş işlemleri ve açılış."),
            ("👥", "Bordro & Danışmanlık", "Personel ve mali danışmanlık."),
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
            ("📋", "Danışmanlık", "İhtiyacınıza uygun çözüm ve süreç planı."),
            ("🧮", "Teklif & Planlama", "Şeffaf fiyat, net takvim ve kapsam."),
            ("🤝", "Takip & Destek", "İş bitiminde de yanınızdayız."),
        ],
    };
    hizmet_kartlari("Hizmetlerimiz", kartlar)
}

/// Tamir, bakım ve teknik servis sektörleri.
fn servis_bolumu(sektor: &str) -> String {
    let kartlar: &[(&str, &str, &str)] = match sektor {
        "Oto Tamir" | "Lastikçi" | "Oto Yıkama" | "Oto Kiralama" => &[
            ("🛠️", "Bakım & Onarım", "Periyodik bakım, fren ve süspansiyon."),
            ("🛞", "Lastik & Balans", "Lastik değişimi, balans ve rot ayarı."),
            ("📅", "Randevu", "Sıra beklemeden teslim ve teslim alma."),
        ],
        "Tesisatçı" | "Elektrikçi" | "Klima" | "Boyacı" | "Marangoz" | "Camcı" | "Çilingir" => &[
            ("🔧", "Keşif & Teklif", "Yerinde inceleme, net fiyat."),
            ("🏠", "Uygulama", "Temiz çalışma, planlanan günde teslim."),
            ("🛡️", "Garanti", "İşçilik garantisi ve acil destek hattı."),
        ],
        "Terzi" | "Kuru Temizleme" | "Ayakkabı Tamiri" => &[
            ("✂️", "Özel İşçilik", "Ölçüye ve isteğe göre düzenleme."),
            ("⏱️", "Hızlı Teslim", "Aynı gün teslim seçeneği."),
            ("🧺", "Bakım & Paketleme", "Temiz, ütülü ve korumalı teslim."),
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
            ("🎟️", "Rezervasyon", "Telefon veya WhatsApp ile yer ayırtma."),
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
    fs::create_dir_all(klasor).map_err(|hata| format!("{klasor} klasörü oluşturulamadı: {hata}"))?;
    let kimlik: String = isletme
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
        .collect();
    let dosya = format!("{klasor}/{}-{kimlik}.html", dosya_slug(&isletme.name));
    fs::write(&dosya, demo_html(isletme, gonderen))
        .map_err(|hata| format!("{dosya} yazılamadı: {hata}"))?;
    Ok(dosya)
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
    sonuc.map(|_| ()).map_err(|hata| format!("{url} açılamadı: {hata}"))
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

        let (liste, siteli) = overpass_isle(json, "İstanbul", "Beşiktaş", "Restoran")
        .expect("yanıt işlenmeliydi");

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
            demo_yolu: None,
            mesaj_hazir: false,
        }
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
        assert_eq!(
            dosya_slug("Şişli Çiçekçi & Kahve"),
            "sisli-cicekci-kahve"
        );
    }

    #[test]
    fn demo_html_ozel_karakterleri_kacirir_ve_tel_ekler() {
        let html = demo_html(&ornek_isletme(), "+90 500 111 22 33");
        assert!(html.contains("Kafe &lt;Deneme&gt;"));
        assert!(html.contains("tel:+905321234567"));
        assert!(html.contains("Demo süresi: 12 saat"));
        assert!(html.contains("Menü"), "restoran demosunda menü bölümü olmalı");
        assert!(
            html.contains("export/embed.html") && html.contains("marker=41.04285,29.00753"),
            "konum varsa gömülü harita olmalı"
        );
        assert!(html.contains("Pzt-Cum 09:00-18:00 · Cmt 10:00-16:00"));
        assert!(html.contains("id=\"acik-durum\""), "açık/kapalı satırı olmalı");
        assert!(html.contains("const program ="), "haftalık program JS'e gömülmeli");
        assert!(
            html.contains("setInterval(durumGuncelle, 60000)"),
            "gösterge dakikada bir güncellenmeli"
        );
        assert!(html.contains("kalanMetni"), "kapanışa kalan süre hesaplanmalı");
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
        assert!(!sade.contains("export/embed.html"), "konum yoksa harita eklenmemeli");
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
        let mesaj = whatsapp_mesaji_ile(
            &ornek_isletme(),
            Some("İşletmeniz şu an kapalı; açılış yarın 09:00."),
        );
        assert!(mesaj.contains("İşletmeniz şu an kapalı; açılış yarın 09:00."));
        assert!(mesaj.contains("çalışma saatleri ve açık/kapalı göstergesi"));

        let durumsuz = whatsapp_mesaji_ile(&ornek_isletme(), None);
        assert!(!durumsuz.contains("İşletmeniz şu an"));
        assert!(durumsuz.contains("12 saat geçerli demo dosyasını"));
        assert!(durumsuz.contains("konum haritası"));
        assert!(!durumsuz.contains("çalışma saatleri ve açık/kapalı"));
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
        assert!(yol.ends_with("kafe-deneme-42.html"), "beklenmeyen dosya: {yol}");
        let icerik = fs::read_to_string(&yol).expect("yazılan dosya okunmalı");
        assert!(icerik.contains("Kafe &lt;Deneme&gt;"));

        let _ = fs::remove_dir_all(&klasor);
    }

    /// Canlı ağ isteği: `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore = "OpenStreetMap'e gerçek istek atar"]
    fn canli_tarama_gercek_isletme_dondurur() {
        let ajan = http_ajani();
        let konum = konum_bul(&ajan, "İstanbul", "Beşiktaş").expect("konum bulunmalı");
        assert!(konum.yaricap_m >= 1_200);
        println!("alan_id: {:?}", konum.alan_id);
        let (liste, siteli) = sitesiz_isletmeler(
            &ajan,
            "İstanbul",
            "Beşiktaş",
            "Restoran",
            &konum,
        )
        .expect("tarama çalışmalı");
        println!("web sitesi olmayan: {}, web sitesi olan: {}", liste.len(), siteli);
        let koordinatli = liste.iter().filter(|isletme| isletme.konum.is_some()).count();
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
        assert!(liste
            .iter()
            .all(|isletme| !isletme.name.is_empty()
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
            assert!(!osm_kurallari(tanim.ad).is_empty(), "eşleme yok: {}", tanim.ad);
            assert!(!demo_bolumu(tanim.ad).is_empty(), "demo bölümü yok: {}", tanim.ad);
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
        assert!(            SEKTORLER.len() >= 200,
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
        }        assert!(gruplar.len() >= 34, "grup sayısı: {}", gruplar.len());
        assert_eq!(sektor_grubu("Giyim"), "Giyim & Aksesuar");
        assert_eq!(sektor_grubu("Spor Salonu"), "Spor & Fitness");
        assert_eq!(sektor_grubu("Düğün Salonu"), "Mekân & Etkinlik");
        assert_eq!(sektor_grubu("Halı"), "Tekstil & Ev Tekstili");
        assert_eq!(sektor_grubu("Diş Kliniği"), "Sağlık");
        assert_eq!(sektor_grubu("Tesisatçı"), "Tesisat & Güvenlik");
        assert_eq!(sektor_grubu("Nakliyat"), "Lojistik & Seyahat");
        assert_eq!(sektor_grubu("Peyzaj"), "Çevre & Bakım");
        assert_eq!(sektor_grubu("Noter"), "Hukuk & Mali");
        assert_eq!(sektor_grubu("Mobilya"), "Ev & Yaşam");
        assert_eq!(sektor_grubu("Elektronik"), "Elektronik & Teknoloji");        assert_eq!(sektor_grubu("Pet Kuaför"), "Evcil Hayvan");
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
        assert!(en_kalabalik <= 15, "en kalabalık grup {en_kalabalik} sektör");
    }

    #[test]
    fn sektor_panelinde_gruplar_ve_sektorler_tutarli() {
        let gruplar = sektor_gruplari();
        assert_eq!(gruplar.len(), 34, "grup sayısı: {}", gruplar.len());

        // Gruplar tekrarsız ve katalog sırasıyla gelmeli.
        let mut sirali = gruplar.clone();
        sirali.dedup();
        assert_eq!(sirali.len(), gruplar.len(), "grup tekrarı olmamalı");

        // Her grubun sektörleri var ve toplam, kataloğun tamamına eşit.
        let toplam: usize = gruplar
            .iter()
            .map(|grup| grup_sektorleri(grup).len())
            .sum();
        assert_eq!(toplam, SEKTORLER.len());
        assert!(!grup_sektorleri("Kuyum & Saat").is_empty());
        assert_eq!(grup_sektorleri("Kuyum & Saat"), vec!["Kuyumcu", "Saatçi"]);

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
            assert_eq!(kota_karari(&mut kota, "2026-11-01", "2026-11", 3, 10), KotaKarari::Izin);
        }
        assert_eq!(
            kota_karari(&mut kota, "2026-11-01", "2026-11", 3, 10),
            KotaKarari::GunlukDolu
        );
        // Ertesi gün günlük sayaç sıfırlanır, aylık devam eder.
        assert_eq!(kota_karari(&mut kota, "2026-11-02", "2026-11", 3, 4), KotaKarari::Izin);
        assert_eq!(kota_karari(&mut kota, "2026-11-02", "2026-11", 3, 4), KotaKarari::AylikDolu);
        // Yeni ay her iki sayacı da sıfırlar.
        assert_eq!(kota_karari(&mut kota, "2026-12-01", "2026-12", 3, 4), KotaKarari::Izin);
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
        let moda = liste.iter().find(|lead| lead.name == "Moda Kuaför").unwrap();
        assert_eq!(moda.phone, "0216 000 00 00");
        assert_eq!(moda.konum, Some((40.98, 29.02)));
        assert_eq!(moda.calisma_saatleri, "Mo-Fr 09:00-18:00; Sa 10:00-14:00; Su off");
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
        assert!(il_geneline_gecilir(0, false), "boş ilçe il geneline geçmeli");
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
