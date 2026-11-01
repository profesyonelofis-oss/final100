//! Kalıcı müşteri defteri.
//!
//! Taramada bulunan işletmeler burada saklanır; uygulama kapanıp açılsa da
//! liste ve iletişim durumu korunur. Profesyonel lead araçlarının ortak
//! özelliği budur: aynı işletmeye ikinci kez teklif götürülmez, "reddetti"
//! işaretlenen işletme kara listeye düşer ve listeye bir daha girmez.
//!
//! Dosya biçimi JSON'dur (`musteri_kayitlari.json`); sürüm alanı ileride
//! şema değişirse taşıma için tutulur.

use std::collections::HashMap;
use std::fs;

/// Varsayılan defter dosyası (çalışma dizininde, .gitignore'da).
pub const DEFTER_DOSYASI: &str = "musteri_kayitlari.json";
/// CSV dışa aktarımının yazıldığı klasör.
pub const CSV_KLASORU: &str = "cikti";
const SURUM: u32 = 1;
/// Dışa aktarım ve test için: A/B farkının anlamlı sayıldığı en küçük gönderim.
pub const ANLAMLI_ORNEKLEM: usize = 30;
/// Defterin bir önceki sürümünün tutulduğu yedek dosya.
pub const YEDEK_DOSYASI: &str = "musteri_kayitlari.yedek.json";

/// Bir işletmeyle ilişkinin bulunduğu aşama.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Durum {
    /// Henüz temas kurulmadı.
    #[default]
    Yeni,
    /// Demo/mesaj hazırlandı ama gönderilmedi.
    MesajHazir,
    /// Mesaj gönderildi, yanıt bekleniyor.
    Gonderildi,
    /// İşletme ilgilendi.
    Ilgilendi,
    /// İşletme istemedi: kara liste, bir daha temas kurulmaz.
    Reddetti,
}

impl Durum {
    pub fn etiket(self) -> &'static str {
        match self {
            Durum::Yeni => "yeni",
            Durum::MesajHazir => "mesaj hazır",
            Durum::Gonderildi => "gönderildi",
            Durum::Ilgilendi => "ilgileniyor",
            Durum::Reddetti => "kara listede",
        }
    }

    /// Sıralama ağırlığı: taze fırsatlar önce, temas edilmişler sonra.
    pub fn sira(self) -> u8 {
        match self {
            Durum::Yeni => 0,
            Durum::MesajHazir => 1,
            Durum::Ilgilendi => 2,
            Durum::Gonderildi => 3,
            Durum::Reddetti => 4,
        }
    }

    /// Kara listedeki işletmeye mesaj hazırlanmaz.
    pub fn kara_listede(self) -> bool {
        self == Durum::Reddetti
    }

    /// CSV'ye yazılan kararlı ad (Türkçe karakter yok, Excel'de bozulmasın).
    pub fn kod(self) -> &'static str {
        match self {
            Durum::Yeni => "yeni",
            Durum::MesajHazir => "mesaj_hazir",
            Durum::Gonderildi => "gonderildi",
            Durum::Ilgilendi => "ilgilendi",
            Durum::Reddetti => "reddetti",
        }
    }
}

/// Mesaj denemesindeki kol (A/B). Aynı işletmeye hep aynı varyant gider;
/// böylece denekler arasında karışma olmaz.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Varyant {
    /// Soruyla açılan klasik mesaj.
    #[default]
    A,
    /// Gerekçeyi öne alan, soru sormayan mesaj.
    B,
}

impl Varyant {
    pub const HEPSI: [Varyant; 2] = [Varyant::A, Varyant::B];

    /// Kayıtta ve CSV'de tutulan kararlı kod.
    pub fn kod(self) -> &'static str {
        match self {
            Varyant::A => "A",
            Varyant::B => "B",
        }
    }

    /// Sayaç dizisindeki yeri (A → 0, B → 1).
    pub fn sira(self) -> usize {
        match self {
            Varyant::A => 0,
            Varyant::B => 1,
        }
    }

    /// Arayüzde gösterilen açıklama.
    pub fn etiket(self) -> &'static str {
        match self {
            Varyant::A => "A · soruyla açılış",
            Varyant::B => "B · gerekçeyle açılış",
        }
    }
}

/// Bir varyantın gönderim ve yanıt sayaçları (panelde gösterilir).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VaryantOzet {
    pub varyant: Varyant,
    /// Varyant atanmış ve gönderilmiş kabul edilen kayıt sayısı.
    pub gonderim: usize,
    pub olumlu: usize,
    pub olumsuz: usize,
    pub bekleyen: usize,
}

impl VaryantOzet {
    /// Örneklem istatistiksel olarak anlamlı mı? Kol başına 30 gönderim
    /// altında oran farkları büyük ölçüde gürültüdür.
    pub fn anlamli(&self) -> bool {
        self.gonderim >= ANLAMLI_ORNEKLEM
    }

    /// Olumlu + olumsuz yanıtların gönderime oranı (0-100).
    pub fn yanit_orani(&self) -> u8 {
        if self.gonderim == 0 {
            return 0;
        }
        let yanit = self.olumlu + self.olumsuz;
        ((yanit * 100) / self.gonderim) as u8
    }

    /// Paneldeki tek satırlık özet: "A: 12 gönderim · %25 yanıt".
    pub fn satir(&self) -> String {
        format!(
            "{}: {} gönderim · %{} yanıt",
            self.varyant.kod(),
            self.gonderim,
            self.yanit_orani()
        )
    }
}

/// Müşteriye sunulan hizmet paketi.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Paket {
    /// Tek sayfa tanıtım sitesi.
    TekSayfa,
    /// Çok sayfalı (menü/galeri/hizmet) site.
    CokSayfa,
    /// Yayında kalan sitenin aylık bakımı.
    Bakim,
}

impl Paket {
    pub const HEPSI: [Paket; 3] = [Paket::TekSayfa, Paket::CokSayfa, Paket::Bakim];

    pub fn etiket(self) -> &'static str {
        match self {
            Paket::TekSayfa => "Tek sayfa",
            Paket::CokSayfa => "Çok sayfa",
            Paket::Bakim => "Bakım",
        }
    }

    /// CSV ve defterde tutulan kararlı kod.
    pub fn kod(self) -> &'static str {
        match self {
            Paket::TekSayfa => "tek_sayfa",
            Paket::CokSayfa => "cok_sayfa",
            Paket::Bakim => "bakim",
        }
    }

    /// Paket seçildiğinde tutar alanına yazılan varsayılan fiyat (TL).
    /// Satıcı bunu her teklif için değiştirebilir.
    pub fn varsayilan_tutar(self) -> u32 {
        match self {
            Paket::TekSayfa => 4500,
            Paket::CokSayfa => 8500,
            Paket::Bakim => 750,
        }
    }
}

/// Teklifin ödeme durumu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Odeme {
    /// Henüz teklif verilmedi.
    #[default]
    Yok,
    /// Teklif verildi, ödeme bekleniyor.
    Bekliyor,
    /// Bir kısmı alındı.
    Kismi,
    /// Tamamı ödendi.
    Odendi,
}

impl Odeme {
    pub const HEPSI: [Odeme; 3] = [Odeme::Bekliyor, Odeme::Kismi, Odeme::Odendi];

    pub fn etiket(self) -> &'static str {
        match self {
            Odeme::Yok => "teklif yok",
            Odeme::Bekliyor => "bekliyor",
            Odeme::Kismi => "kısmi",
            Odeme::Odendi => "ödendi",
        }
    }

    pub fn kod(self) -> &'static str {
        match self {
            Odeme::Yok => "yok",
            Odeme::Bekliyor => "bekliyor",
            Odeme::Kismi => "kismi",
            Odeme::Odendi => "odendi",
        }
    }
}

/// Teklif ve tahsilat özeti (panelde gösterilir).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CiroOzet {
    /// Tamamı ödenmiş tekliflerin toplamı (TL).
    pub odenen: u32,
    /// Ödemesi beklenen tekliflerin toplamı (TL).
    pub bekleyen: u32,
    /// Teklif verilmiş kayıt sayısı.
    pub teklif_sayisi: usize,
    /// Ödemesi tamamlanmış kayıt sayısı.
    pub odenen_adet: usize,
}

/// Bir gönderim olayı: hangi işletmeye hangi gün mesaj hazırlandı.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Gonderim {
    tarih: String,
    anahtar: String,
}

/// Defterdeki tek müşteri kaydı.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Musteri {
    pub anahtar: String,
    pub ad: String,
    pub telefon: String,
    pub adres: String,
    pub kategori: String,
    pub kaynak: String,
    /// Web varlığı: "yok" / "sosyal" / "ucretsiz".
    pub site: String,
    pub site_linki: String,
    /// Satın alma potansiyeli puanı (0-100), son taramadaki değer.
    pub puan: u8,
    #[serde(default)]
    pub durum: Durum,
    pub ilk_gorulme: String,
    pub son_gorulme: String,
    #[serde(default)]
    pub son_temas: Option<String>,
    /// En son mesaj gönderim tarihi. Günlük gönderim freni ve takip
    /// kuyruğu bu tarihe bakar (`son_temas` ilk temastır, değişmez).
    #[serde(default)]
    pub son_gonderim: Option<String>,
    #[serde(default)]
    pub notlar: String,
    /// Bu işletmenin mesaj varyantı (eski kayıtlarda boş; sonraki taramada atanır).
    #[serde(default)]
    pub varyant: Option<Varyant>,
    /// Yayındaki demo adresi. Takip mesajında güncel linki paylaşmak için
    /// tarama listesi dışındaki kayıtlarda da saklanır.
    #[serde(default)]
    pub yayin_linki: Option<String>,
    /// Teklif edilen paket (teklif verilmediyse boş).
    #[serde(default)]
    pub paket: Option<Paket>,
    /// Teklifin tutarı (TL). Paket seçilince varsayılanı yazılır.
    #[serde(default)]
    pub teklif_tutari: Option<u32>,
    #[serde(default)]
    pub odeme: Odeme,
    /// Teklifin verildiği gün.
    #[serde(default)]
    pub teklif_tarihi: Option<String>,
}

/// Defterin özet sayaçları (arayüzde ve günlükte gösterilir).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ozet {
    pub toplam: usize,
    pub yeni: usize,
    /// Mesaj hazırlanmış ama henüz gönderilmemiş kayıtlar.
    pub hazir: usize,
    pub gonderilen: usize,
    pub ilgilenen: usize,
    pub kara_liste: usize,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Dosya {
    #[serde(default)]
    surum: u32,
    #[serde(default)]
    musteriler: Vec<Musteri>,
    /// Gönderim olayları: günlük fren bu listeden sayılır, aynı işletmeye
    /// aynı gün ikinci kez sayılmaz. Son 400 olay tutulur.
    #[serde(default)]
    gonderim_gunleri: Vec<Gonderim>,
}

/// Kalıcı defter: kayıtlar + anahtar→indeks eşlemesi.
#[derive(Default)]
pub struct Defter {
    dosya: Dosya,
    indeks: HashMap<String, usize>,
}

impl Defter {
    /// Dosyadan yükler; dosya yoksa veya bozuksa boş defter döner.
    pub fn yukle() -> Self {
        Self::yukle_yoldan(DEFTER_DOSYASI)
    }

    pub fn yukle_yoldan(yol: &str) -> Self {
        let metin = fs::read_to_string(yol).unwrap_or_default();
        let mut defter = Self::metinden(&metin);
        defter.dosya.surum = SURUM;
        defter
    }

    /// JSON metninden defter kurar (bozuk metin → boş defter).
    pub fn metinden(metin: &str) -> Self {
        let dosya: Dosya = serde_json::from_str(metin).unwrap_or_default();
        let mut defter = Self {
            dosya,
            indeks: HashMap::new(),
        };
        defter.indeks_yenile();
        defter
    }

    pub fn kaydet(&self) {
        if let Ok(metin) = serde_json::to_string_pretty(&self.dosya) {
            // Yazmadan önceki hâli yedekte tutulur: bozuk/eksik bir yazma
            // kara listeyi ve gönderim geçmişini kaybettirmesin.
            if fs::metadata(DEFTER_DOSYASI)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            {
                let _ = fs::copy(DEFTER_DOSYASI, YEDEK_DOSYASI);
            }
            let _ = fs::write(DEFTER_DOSYASI, metin);
        }
    }

    fn indeks_yenile(&mut self) {
        self.indeks.clear();
        for (sira, musteri) in self.dosya.musteriler.iter().enumerate() {
            self.indeks.insert(musteri.anahtar.clone(), sira);
        }
    }

    /// Tarama sonucunu deftere işler ve işletmenin güncel durumunu döndürür.
    /// Yeni kayıt eklenir, var olanın iletişim bilgileri ve puanı tazelenir;
    /// durum (gönderildi/reddetti...) korunur.
    #[allow(clippy::too_many_arguments)]
    pub fn isle(
        &mut self,
        anahtar: &str,
        ad: &str,
        telefon: &str,
        adres: &str,
        kategori: &str,
        kaynak: &str,
        site: &str,
        site_linki: &str,
        puan: u8,
        tarih: &str,
    ) -> Durum {
        if let Some(&sira) = self.indeks.get(anahtar) {
            // Varyant eksikse (eski kayıt) dengeli dağıtımla tamamlanır; ödünç
            // çakışmasın diye önce hesaplanır.
            let eksik_varyant = self.dosya.musteriler[sira].varyant.is_none();
            let tamamla = eksik_varyant.then(|| varyant_sec(&self.dosya.musteriler, kategori));
            let musteri = &mut self.dosya.musteriler[sira];
            musteri.ad = ad.to_string();
            musteri.telefon = telefon.to_string();
            musteri.adres = adres.to_string();
            musteri.kategori = kategori.to_string();
            musteri.kaynak = kaynak.to_string();
            musteri.site = site.to_string();
            musteri.site_linki = site_linki.to_string();
            musteri.puan = puan;
            musteri.son_gorulme = tarih.to_string();
            if let Some(varyant) = tamamla {
                musteri.varyant = Some(varyant);
            }
            return musteri.durum;
        }
        let varyant = varyant_sec(&self.dosya.musteriler, kategori);
        let musteri = Musteri {
            anahtar: anahtar.to_string(),
            ad: ad.to_string(),
            telefon: telefon.to_string(),
            adres: adres.to_string(),
            kategori: kategori.to_string(),
            kaynak: kaynak.to_string(),
            site: site.to_string(),
            site_linki: site_linki.to_string(),
            puan,
            durum: Durum::Yeni,
            ilk_gorulme: tarih.to_string(),
            son_gorulme: tarih.to_string(),
            son_temas: None,
            son_gonderim: None,
            notlar: String::new(),
            varyant: Some(varyant),
            yayin_linki: None,
            paket: None,
            teklif_tutari: None,
            odeme: Odeme::Yok,
            teklif_tarihi: None,
        };
        self.indeks
            .insert(anahtar.to_string(), self.dosya.musteriler.len());
        self.dosya.musteriler.push(musteri);
        Durum::Yeni
    }

    /// İşletmenin durumunu değiştirir. İlk temas tarihi yalnızca bir kez yazılır.
    pub fn durum_ayarla(&mut self, anahtar: &str, durum: Durum, tarih: &str) {
        if let Some(&sira) = self.indeks.get(anahtar) {
            let musteri = &mut self.dosya.musteriler[sira];
            musteri.durum = durum;
            if musteri.son_temas.is_none() && durum != Durum::Yeni {
                musteri.son_temas = Some(tarih.to_string());
            }
        }
    }

    /// Bir mesaj gönderimini kaydeder: olay tarihi listeye eklenir ve kaydın
    /// son gönderim tarihi tazelenir. Günlük fren defterde tutulduğu için
    /// uygulama yeniden başlasa da sayaç sıfırlanmaz.
    pub fn gonderim_kaydet(&mut self, anahtar: &str, tarih: &str) {
        if self.bugun_gonderildi_mi(anahtar, tarih) {
            return;
        }
        self.dosya.gonderim_gunleri.push(Gonderim {
            tarih: tarih.to_string(),
            anahtar: anahtar.to_string(),
        });
        let sinir = self.dosya.gonderim_gunleri.len().saturating_sub(400);
        if sinir > 0 {
            self.dosya.gonderim_gunleri.drain(..sinir);
        }
        if let Some(&sira) = self.indeks.get(anahtar) {
            self.dosya.musteriler[sira].son_gonderim = Some(tarih.to_string());
        }
    }

    /// Bu işletmeye verilen günde zaten mesaj hazırlanmış mı?
    pub fn bugun_gonderildi_mi(&self, anahtar: &str, tarih: &str) -> bool {
        self.dosya
            .gonderim_gunleri
            .iter()
            .any(|olay| olay.anahtar == anahtar && olay.tarih == tarih)
    }

    /// Verilen günde yapılan gönderim sayısı.
    pub fn gun_gonderim(&self, tarih: &str) -> usize {
        self.dosya
            .gonderim_gunleri
            .iter()
            .filter(|olay| olay.tarih == tarih)
            .count()
    }

    /// Yayındaki demo adresini kayda işler.
    pub fn link_ayarla(&mut self, anahtar: &str, link: &str) {
        if let Some(&sira) = self.indeks.get(anahtar) {
            self.dosya.musteriler[sira].yayin_linki = Some(link.to_string());
        }
    }

    /// Teklifi kayda yazar. Tutar verilmezse paketin varsayılanı kullanılır;
    /// teklif tarihi yalnızca ilk teklifte yazılır.
    pub fn teklif_ayarla(&mut self, anahtar: &str, paket: Paket, tutar: u32, tarih: &str) {
        if let Some(&sira) = self.indeks.get(anahtar) {
            let musteri = &mut self.dosya.musteriler[sira];
            musteri.paket = Some(paket);
            musteri.teklif_tutari = Some(tutar);
            if musteri.teklif_tarihi.is_none() {
                musteri.teklif_tarihi = Some(tarih.to_string());
            }
            if musteri.odeme == Odeme::Yok {
                musteri.odeme = Odeme::Bekliyor;
            }
        }
    }

    /// Ödeme durumunu değiştirir (bekliyor / kısmi / ödendi).
    pub fn odeme_ayarla(&mut self, anahtar: &str, odeme: Odeme) {
        if let Some(&sira) = self.indeks.get(anahtar) {
            self.dosya.musteriler[sira].odeme = odeme;
        }
    }

    /// Tahsilat özeti: ödenen, bekleyen ve teklif sayısı.
    pub fn ciro_ozeti(&self) -> CiroOzet {
        let mut ozet = CiroOzet::default();
        for kayit in &self.dosya.musteriler {
            if kayit.paket.is_none() {
                continue;
            }
            ozet.teklif_sayisi += 1;
            let tutar = kayit.teklif_tutari.unwrap_or(0);
            match kayit.odeme {
                Odeme::Odendi => {
                    ozet.odenen += tutar;
                    ozet.odenen_adet += 1;
                }
                Odeme::Bekliyor | Odeme::Kismi => ozet.bekleyen += tutar,
                Odeme::Yok => {}
            }
        }
        ozet
    }

    /// Görüşme notunu yazar (paneldeki serbest metin alanı).
    pub fn not_ayarla(&mut self, anahtar: &str, not: &str) {
        if let Some(&sira) = self.indeks.get(anahtar) {
            self.dosya.musteriler[sira].notlar = not.to_string();
        }
    }

    /// Takip zamanı gelmiş işletmeler: mesaj gönderilmiş, `gun` günden fazla
    /// geçmiş ve hâlâ yanıt bekleniyor. En eski temas önce gelir.
    pub fn takip_edilecekler(&self, bugun: &str, gun: i64) -> Vec<&Musteri> {
        let mut liste: Vec<&Musteri> = self
            .dosya
            .musteriler
            .iter()
            .filter(|kayit| kayit.durum == Durum::Gonderildi)
            .filter(|kayit| {
                kayit
                    .son_gonderim
                    .as_deref()
                    .or(kayit.son_temas.as_deref())
                    .and_then(|tarih| gun_farki(tarih, bugun))
                    .map(|gecen| gecen >= gun)
                    .unwrap_or(false)
            })
            .collect();
        liste.sort_by_key(|kayit| {
            kayit
                .son_gonderim
                .clone()
                .or_else(|| kayit.son_temas.clone())
                .unwrap_or_default()
        });
        liste
    }

    pub fn kayitlar(&self) -> &[Musteri] {
        &self.dosya.musteriler
    }

    pub fn kayit(&self, anahtar: &str) -> Option<&Musteri> {
        self.indeks
            .get(anahtar)
            .map(|&sira| &self.dosya.musteriler[sira])
    }

    /// İşletmeye atanmış varyant; kayıt yoksa varsayılan A.
    pub fn varyant(&self, anahtar: &str) -> Varyant {
        self.kayit(anahtar)
            .and_then(|kayit| kayit.varyant)
            .unwrap_or_default()
    }

    /// Varyant başına gönderim ve yanıt sayaçları.
    /// Yalnızca gönderildiği işaretlenmiş kayıtlar sayılır; branş atanıp
    /// gönderilmemiş kayıtlar oranı bozmaz.
    pub fn varyant_ozeti(&self) -> [VaryantOzet; 2] {
        self.varyant_ozeti_filtreli(|_| true)
    }

    /// Yalnızca bir sektördeki kayıtların A/B sayaçları: sektöre göre
    /// farklı mesaj açılışı daha iyi çalışıyor olabilir.
    pub fn varyant_ozeti_kategori(&self, kategori: &str) -> [VaryantOzet; 2] {
        self.varyant_ozeti_filtreli(|kayit| kayit.kategori == kategori)
    }

    fn varyant_ozeti_filtreli(&self, kabul: impl Fn(&Musteri) -> bool) -> [VaryantOzet; 2] {
        let mut ozet = [
            VaryantOzet {
                varyant: Varyant::A,
                ..VaryantOzet::default()
            },
            VaryantOzet {
                varyant: Varyant::B,
                ..VaryantOzet::default()
            },
        ];
        for kayit in self.dosya.musteriler.iter().filter(|kayit| kabul(kayit)) {
            let Some(varyant) = kayit.varyant else {
                continue;
            };
            let sira = varyant.sira();
            match kayit.durum {
                Durum::Yeni | Durum::MesajHazir => continue,
                Durum::Gonderildi => ozet[sira].bekleyen += 1,
                Durum::Ilgilendi => ozet[sira].olumlu += 1,
                Durum::Reddetti => ozet[sira].olumsuz += 1,
            }
            ozet[sira].gonderim += 1;
        }
        ozet
    }

    pub fn ozet(&self) -> Ozet {
        let mut ozet = Ozet {
            toplam: self.dosya.musteriler.len(),
            ..Ozet::default()
        };
        for musteri in &self.dosya.musteriler {
            match musteri.durum {
                Durum::Yeni => ozet.yeni += 1,
                Durum::MesajHazir => ozet.hazir += 1,
                Durum::Gonderildi => ozet.gonderilen += 1,
                Durum::Ilgilendi => ozet.ilgilenen += 1,
                Durum::Reddetti => ozet.kara_liste += 1,
            }
        }
        ozet
    }

    /// Defterin tamamını `cikti/musteriler.csv` dosyasına yazar ve yolu döndürür.
    pub fn csv_kaydet(&self) -> std::io::Result<String> {
        fs::create_dir_all(CSV_KLASORU)?;
        let yol = format!("{CSV_KLASORU}/musteriler.csv");
        fs::write(&yol, self.csv())?;
        Ok(yol)
    }

    /// Excel'in Türkçe yerel ayarında doğru açılan CSV (';' ayraçlı).
    pub fn csv(&self) -> String {
        let mut cikti = String::from(
            "ad;telefon;adres;kategori;site;site_linki;puan;durum;ilk_gorulme;son_gorulme;son_temas;son_gonderim;kaynak;notlar;varyant;paket;teklif_tutari;odeme;teklif_tarihi\n",
        );
        for musteri in &self.dosya.musteriler {
            let alanlar = [
                musteri.ad.as_str(),
                musteri.telefon.as_str(),
                musteri.adres.as_str(),
                musteri.kategori.as_str(),
                musteri.site.as_str(),
                musteri.site_linki.as_str(),
                &musteri.puan.to_string(),
                musteri.durum.kod(),
                musteri.ilk_gorulme.as_str(),
                musteri.son_gorulme.as_str(),
                musteri.son_temas.as_deref().unwrap_or(""),
                musteri.son_gonderim.as_deref().unwrap_or(""),
                musteri.kaynak.as_str(),
                musteri.notlar.as_str(),
                musteri.varyant.map(|v| v.kod()).unwrap_or(""),
                musteri.paket.map(|paket| paket.kod()).unwrap_or(""),
                &musteri
                    .teklif_tutari
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                musteri.odeme.kod(),
                musteri.teklif_tarihi.as_deref().unwrap_or(""),
            ];
            let satir: Vec<String> = alanlar.iter().map(|alan| csv_alan(alan)).collect();
            cikti.push_str(&satir.join(";"));
            cikti.push('\n');
        }
        cikti
    }
}

/// İki tarih arasındaki gün farkı; çözülemeyen tarihte `None`.
pub fn gun_farki(eski: &str, yeni: &str) -> Option<i64> {
    let once = chrono::NaiveDate::parse_from_str(eski, "%Y-%m-%d").ok()?;
    let sonra = chrono::NaiveDate::parse_from_str(yeni, "%Y-%m-%d").ok()?;
    Some((sonra - once).num_days())
}

/// CSV alanı: ayraç, tırnak veya satır sonu varsa tırnak içine alınır.
fn csv_alan(metin: &str) -> String {
    let kacis_gerekli = metin
        .chars()
        .any(|harf| matches!(harf, ';' | '"' | '\n' | '\r'));
    if kacis_gerekli {
        format!("\"{}\"", metin.replace('"', "\"\""))
    } else {
        metin.to_string()
    }
}

/// Kayıt anahtarı: işletmeyi taramalar arasında eşleştiren kararlı kimlik.
/// OSM kaydı için node/way kimliği, Google için place ID, yoksa ad+koordinat.
/// Gönderim sayısı az olan varyantı seçer; eşitlikte A'ya döner.
/// Denge **sektör içinde** kurulur: restoran müşterilerinde A/B eşit dağılır,
/// böylece "hangi sektörde hangi açılış daha iyi yanıt alıyor" sorusu da
/// cevaplanabilir hâle gelir.
fn varyant_sec(kayitlar: &[Musteri], kategori: &str) -> Varyant {
    let a = kayitlar
        .iter()
        .filter(|kayit| kayit.kategori == kategori)
        .filter(|kayit| kayit.varyant == Some(Varyant::A))
        .count();
    let b = kayitlar
        .iter()
        .filter(|kayit| kayit.kategori == kategori)
        .filter(|kayit| kayit.varyant == Some(Varyant::B))
        .count();
    if b < a {
        Varyant::B
    } else {
        Varyant::A
    }
}

pub fn anahtar_uret(kaynak: &str, ad: &str, konum: Option<(f64, f64)>) -> String {
    if let Some(kalan) = kaynak.split("openstreetmap.org/").nth(1) {
        return format!("osm:{}", kalan.trim_end_matches('/'));
    }
    if let Some(kalan) = kaynak.split("place_id:").nth(1) {
        return format!("google:{}", kalan.trim_end_matches('/'));
    }
    match konum {
        Some((enlem, boylam)) => format!("ad:{}|{enlem:.4},{boylam:.4}", ad.to_lowercase()),
        None => format!("ad:{}", ad.to_lowercase()),
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn anahtar_kaynaga_gore_uretiltir() {
        assert_eq!(
            anahtar_uret("https://www.openstreetmap.org/node/42", "Kafe", None),
            "osm:node/42"
        );
        assert_eq!(
            anahtar_uret(
                "https://www.google.com/maps/place/?q=place_id:ChIJ123",
                "Kafe",
                None
            ),
            "google:ChIJ123"
        );
        assert_eq!(
            anahtar_uret("bilinmeyen", "Kafe", Some((41.04285, 29.00753))),
            "ad:kafe|41.0429,29.0075"
        );
    }

    #[test]
    fn ikinci_tarama_durumu_korur_ve_mukerrer_eklemez() {
        let mut defter = Defter::default();
        let anahtar = "osm:node/42";
        let durum = defter.isle(
            anahtar,
            "Kafe",
            "+90 532 111 22 33",
            "Beşiktaş / İstanbul",
            "Kafe",
            "https://www.openstreetmap.org/node/42",
            "sosyal",
            "https://instagram.com/kafe",
            82,
            "2026-10-01",
        );
        assert_eq!(durum, Durum::Yeni);
        defter.durum_ayarla(anahtar, Durum::Gonderildi, "2026-10-02");

        // Aynı işletme tekrar tarandı: puan ve iletişim bilgisi tazelenir,
        // durum korunur, kayıt ikinci kez eklenmez.
        let durum = defter.isle(
            anahtar,
            "Kafe",
            "+90 532 999 88 77",
            "Beşiktaş / İstanbul",
            "Kafe",
            "https://www.openstreetmap.org/node/42",
            "sosyal",
            "https://instagram.com/kafe",
            90,
            "2026-10-05",
        );
        assert_eq!(durum, Durum::Gonderildi);
        assert_eq!(defter.ozet().toplam, 1);
        let kayit = defter.kayit(anahtar).expect("kayıt bulunmalı");
        assert_eq!(kayit.telefon, "+90 532 999 88 77");
        assert_eq!(kayit.puan, 90);
        assert_eq!(kayit.son_gorulme, "2026-10-05");
        assert_eq!(kayit.son_temas.as_deref(), Some("2026-10-02"));
    }

    #[test]
    fn kara_liste_ozet_ve_durum_sirasi() {
        let mut defter = Defter::default();
        for (sira, ad) in ["A", "B", "C"].iter().enumerate() {
            let anahtar = format!("osm:node/{sira}");
            defter.isle(
                &anahtar,
                ad,
                "",
                "",
                "Kafe",
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
        }
        defter.durum_ayarla("osm:node/0", Durum::Gonderildi, "2026-10-02");
        defter.durum_ayarla("osm:node/1", Durum::Reddetti, "2026-10-02");

        let ozet = defter.ozet();
        assert_eq!(ozet.toplam, 3);
        assert_eq!(ozet.yeni, 1);
        assert_eq!(ozet.gonderilen, 1);
        assert_eq!(ozet.hazir, 0);
        assert_eq!(ozet.kara_liste, 1);
        assert!(defter
            .kayit("osm:node/1")
            .expect("kayıt")
            .durum
            .kara_listede());
        assert!(!defter
            .kayit("osm:node/2")
            .expect("kayıt")
            .durum
            .kara_listede());
        assert!(Durum::Yeni.sira() < Durum::Gonderildi.sira());
        assert!(Durum::Reddetti.sira() > Durum::Gonderildi.sira());
    }

    #[test]
    fn csv_ayraci_ve_tirnagi_kacirir() {
        let mut defter = Defter::default();
        defter.isle(
            "osm:node/1",
            "Kafe \"Mavi\"; Şube",
            "+90 212 000 00 00",
            "Cadde No:1",
            "Kafe",
            "https://www.openstreetmap.org/node/1",
            "yok",
            "",
            70,
            "2026-10-01",
        );
        let csv = defter.csv();
        let satir = csv.lines().nth(1).expect("veri satırı olmalı");
        assert!(
            satir.starts_with("\"Kafe \"\"Mavi\"\"; Şube\";"),
            "ayraç ve tırnak kaçışlanmalı: {satir}"
        );
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn varyant_dengeli_dagitilir_ve_sabit_kalir() {
        let mut defter = Defter::default();
        let mut dagitim = [0usize; 2];
        for sira in 0..6 {
            let anahtar = format!("osm:node/{sira}");
            defter.isle(
                &anahtar,
                "Kafe",
                "",
                "",
                "Kafe",
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
            dagitim[defter.varyant(&anahtar).sira()] += 1;
        }
        assert_eq!(dagitim, [3, 3], "varyantlar dengeli dağıtılmalı");

        // Aynı işletme tekrar tarandığında varyantı değişmez.
        let once = defter.varyant("osm:node/0");
        defter.isle(
            "osm:node/0",
            "Kafe",
            "",
            "",
            "Kafe",
            "",
            "yok",
            "",
            60,
            "2026-10-02",
        );
        assert_eq!(defter.varyant("osm:node/0"), once);
    }

    #[test]
    fn varyant_ozeti_yalnizca_gonderilenleri_sayar() {
        let mut defter = Defter::default();
        let anahtarlar: Vec<String> = (0..4).map(|sira| format!("osm:node/{sira}")).collect();
        for anahtar in &anahtarlar {
            defter.isle(
                anahtar,
                "Kafe",
                "",
                "",
                "Kafe",
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
        }
        // Henüz gönderilmemiş kayıtlar oranı bozmaz.
        assert!(defter.varyant_ozeti().iter().all(|ozet| ozet.gonderim == 0));

        // Her kolda bir yanıt, bir beklemede.
        let mut kolda = [0usize; 2];
        for anahtar in &anahtarlar {
            let sira = defter.varyant(anahtar).sira();
            kolda[sira] += 1;
            let durum = if kolda[sira] == 1 {
                Durum::Ilgilendi
            } else {
                Durum::Gonderildi
            };
            defter.durum_ayarla(anahtar, durum, "2026-10-02");
        }

        for kol in defter.varyant_ozeti() {
            assert_eq!(kol.gonderim, 2, "{kol:?}");
            assert_eq!(kol.olumlu, 1);
            assert_eq!(kol.olumsuz, 0);
            assert_eq!(kol.bekleyen, 1);
            assert_eq!(kol.yanit_orani(), 50);
        }
    }

    #[test]
    fn csv_varyanti_disa_aktarir() {
        let mut defter = Defter::default();
        defter.isle(
            "osm:node/1",
            "Kafe",
            "",
            "",
            "Kafe",
            "",
            "yok",
            "",
            70,
            "2026-10-01",
        );
        let csv = defter.csv();
        let baslik = csv.lines().next().unwrap_or_default();
        // Varyant sütunu paket sütunlarından önce gelir; sıra korunmalı.
        assert!(
            baslik.contains(";varyant;paket;"),
            "başlıkta varyant sütunu olmalı: {baslik}"
        );
        let satir = csv.lines().nth(1).expect("veri satırı");
        assert!(satir.contains(";A;") || satir.contains(";B;"), "{satir}");
    }

    #[test]
    fn eski_kayit_varyantsiz_yuklenir() {
        // Varyant alanı olmayan eski defter bozulmadan okunmalı.
        let metin = r#"{"surum":1,"musteriler":[{"anahtar":"osm:node/1","ad":"Kafe","telefon":"","adres":"","kategori":"Kafe","kaynak":"","site":"yok","site_linki":"","puan":40,"durum":"Gonderildi","ilk_gorulme":"2026-10-01","son_gorulme":"2026-10-01"}]}"#;
        let defter = Defter::metinden(metin);
        assert_eq!(defter.ozet().toplam, 1);
        assert_eq!(defter.kayit("osm:node/1").expect("kayıt").puan, 40);
        // Varyantsız kayıt varsayılan kola düşer, gönderim sayılmaz.
        assert_eq!(defter.varyant("osm:node/1"), Varyant::A);
        assert_eq!(defter.varyant_ozeti()[0].gonderim, 0);
    }

    #[test]
    fn gunluk_gonderim_ayni_gun_tekrar_sayilmaz() {
        let mut defter = Defter::default();
        defter.isle(
            "osm:node/1",
            "Kafe",
            "",
            "",
            "Kafe",
            "",
            "yok",
            "",
            50,
            "2026-10-01",
        );
        defter.isle(
            "osm:node/2",
            "Fırın",
            "",
            "",
            "Fırın",
            "",
            "yok",
            "",
            50,
            "2026-10-01",
        );
        defter.gonderim_kaydet("osm:node/1", "2026-10-01");
        // Aynı işletme, aynı gün: hak ikinci kez düşmez.
        defter.gonderim_kaydet("osm:node/1", "2026-10-01");
        assert_eq!(defter.gun_gonderim("2026-10-01"), 1);
        assert!(defter.bugun_gonderildi_mi("osm:node/1", "2026-10-01"));

        defter.gonderim_kaydet("osm:node/2", "2026-10-01");
        defter.gonderim_kaydet("osm:node/1", "2026-10-02");
        assert_eq!(defter.gun_gonderim("2026-10-01"), 2);
        assert_eq!(defter.gun_gonderim("2026-10-02"), 1);
        assert_eq!(
            defter
                .kayit("osm:node/1")
                .expect("kayıt")
                .son_gonderim
                .as_deref(),
            Some("2026-10-02")
        );
        // İlk temas tarihi ayrı tutulur.
        assert!(defter
            .kayit("osm:node/1")
            .expect("kayıt")
            .son_temas
            .is_none());
    }

    #[test]
    fn takip_kuyrugu_zaman_gelenleri_listeler() {
        let mut defter = Defter::default();
        for sira in 0..3 {
            defter.isle(
                &format!("osm:node/{sira}"),
                "Kafe",
                "",
                "",
                "Kafe",
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
        }
        // 4 gün önce gönderildi: takip sırası geldi.
        defter.durum_ayarla("osm:node/0", Durum::Gonderildi, "2026-10-01");
        defter.gonderim_kaydet("osm:node/0", "2026-10-01");
        // Dün gönderildi: henüz sıra gelmedi.
        defter.durum_ayarla("osm:node/1", Durum::Gonderildi, "2026-10-04");
        defter.gonderim_kaydet("osm:node/1", "2026-10-04");
        // Yanıt verdi: takip edilmez.
        defter.durum_ayarla("osm:node/2", Durum::Ilgilendi, "2026-10-01");
        defter.gonderim_kaydet("osm:node/2", "2026-10-01");

        let kuyruk = defter.takip_edilecekler("2026-10-05", 3);
        assert_eq!(kuyruk.len(), 1);
        assert_eq!(kuyruk[0].anahtar, "osm:node/0");
        // Yanıt beklemeyenler kuyruğa hiç girmez.
        assert!(defter.takip_edilecekler("2026-10-05", 30).is_empty());
    }

    #[test]
    fn varyant_sektor_icinde_dengelenir() {
        let mut defter = Defter::default();
        let mut sektor_dagilim: HashMap<&str, [usize; 2]> = HashMap::new();
        for (sira, kategori) in ["Kafe", "Kafe", "Kafe", "Kafe", "Fırın", "Fırın"]
            .iter()
            .enumerate()
        {
            let anahtar = format!("osm:node/{sira}");
            defter.isle(
                &anahtar,
                "İşletme",
                "",
                "",
                kategori,
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
            let giris = sektor_dagilim.entry(kategori).or_default();
            giris[defter.varyant(&anahtar).sira()] += 1;
        }
        // Her sektör kendi içinde dengeli: 4 kafe → 2/2, 2 fırın → 1/1.
        assert_eq!(sektor_dagilim["Kafe"], [2, 2]);
        assert_eq!(sektor_dagilim["Fırın"], [1, 1]);
    }

    #[test]
    fn not_ve_yayin_linki_kayda_yazilir() {
        let mut defter = Defter::default();
        defter.isle(
            "osm:node/1",
            "Kafe",
            "",
            "",
            "Kafe",
            "",
            "yok",
            "",
            50,
            "2026-10-01",
        );
        defter.not_ayarla("osm:node/1", "fiyat sordu; pazartesi arayacak");
        defter.link_ayarla("osm:node/1", "https://ornek.github.io/r/demos/a.html");
        let kayit = defter.kayit("osm:node/1").expect("kayıt");
        assert_eq!(kayit.notlar, "fiyat sordu; pazartesi arayacak");
        assert_eq!(
            kayit.yayin_linki.as_deref(),
            Some("https://ornek.github.io/r/demos/a.html")
        );
        // Not, noktalı virgül içerse de CSV'de tek alan olarak kaçışlanır.
        let satir = defter.csv();
        assert!(
            satir.contains("\"fiyat sordu; pazartesi arayacak\""),
            "{satir}"
        );
        assert!(satir.contains(";son_gonderim;"), "başlıkta sütun olmalı");
    }

    #[test]
    fn teklif_ve_odeme_ciroyu_hesaplar() {
        let mut defter = Defter::default();
        for sira in 0..3 {
            defter.isle(
                &format!("osm:node/{sira}"),
                "Kafe",
                "",
                "",
                "Kafe",
                "",
                "yok",
                "",
                50,
                "2026-10-01",
            );
        }
        // Teklif verilmemiş kayıt ciroya girmez.
        assert_eq!(defter.ciro_ozeti(), CiroOzet::default());

        defter.teklif_ayarla("osm:node/0", Paket::CokSayfa, 8500, "2026-10-02");
        defter.teklif_ayarla("osm:node/1", Paket::Bakim, 750, "2026-10-03");
        let ciro = defter.ciro_ozeti();
        assert_eq!(ciro.teklif_sayisi, 2);
        assert_eq!(ciro.odenen, 0);
        assert_eq!(ciro.bekleyen, 9250);
        // Teklif verilince ödeme "bekliyor"a geçer.
        assert_eq!(
            defter.kayit("osm:node/0").expect("kayıt").odeme,
            Odeme::Bekliyor
        );

        defter.odeme_ayarla("osm:node/0", Odeme::Odendi);
        let ciro = defter.ciro_ozeti();
        assert_eq!(ciro.odenen, 8500);
        assert_eq!(ciro.odenen_adet, 1);
        assert_eq!(ciro.bekleyen, 750);
    }

    #[test]
    fn teklif_ilk_tarihi_korur_ve_csvye_yazilir() {
        let mut defter = Defter::default();
        defter.isle(
            "osm:node/1",
            "Kafe",
            "",
            "",
            "Kafe",
            "",
            "yok",
            "",
            50,
            "2026-10-01",
        );
        defter.teklif_ayarla("osm:node/1", Paket::TekSayfa, 4500, "2026-10-02");
        // Paket değişse de ilk teklif tarihi korunur.
        defter.teklif_ayarla("osm:node/1", Paket::CokSayfa, 9000, "2026-10-05");
        let kayit = defter.kayit("osm:node/1").expect("kayıt");
        assert_eq!(kayit.paket, Some(Paket::CokSayfa));
        assert_eq!(kayit.teklif_tutari, Some(9000));
        assert_eq!(kayit.teklif_tarihi.as_deref(), Some("2026-10-02"));

        let csv = defter.csv();
        let baslik = csv.lines().next().unwrap_or_default();
        assert!(
            baslik.ends_with("paket;teklif_tutari;odeme;teklif_tarihi"),
            "{baslik}"
        );
        let satir = csv.lines().nth(1).expect("veri satırı");
        assert!(
            satir.contains(";cok_sayfa;9000;bekliyor;2026-10-02"),
            "{satir}"
        );
    }

    #[test]
    fn eski_kayit_teklif_alanlari_olmadan_yuklenir() {
        let metin = r#"{"surum":1,"musteriler":[{"anahtar":"osm:node/1","ad":"Kafe","telefon":"","adres":"","kategori":"Kafe","kaynak":"","site":"yok","site_linki":"","puan":40,"durum":"Ilgilendi","ilk_gorulme":"2026-10-01","son_gorulme":"2026-10-01"}]}"#;
        let defter = Defter::metinden(metin);
        let kayit = defter.kayit("osm:node/1").expect("kayıt");
        assert_eq!(kayit.paket, None);
        assert_eq!(kayit.odeme, Odeme::Yok);
        assert_eq!(defter.ciro_ozeti(), CiroOzet::default());
    }

    #[test]
    fn orneklem_esigi_otuz_gonderim() {
        let mut ozet = VaryantOzet {
            varyant: Varyant::A,
            gonderim: ANLAMLI_ORNEKLEM - 1,
            ..VaryantOzet::default()
        };
        assert!(!ozet.anlamli());
        ozet.gonderim = ANLAMLI_ORNEKLEM;
        assert!(ozet.anlamli());
    }

    #[test]
    fn bozuk_dosya_bos_defter_dondurur() {
        let defter = Defter::metinden("{bozuk json");
        assert_eq!(defter.ozet().toplam, 0);
        let defter = Defter::metinden("");
        assert_eq!(defter.ozet().toplam, 0);
    }
}
