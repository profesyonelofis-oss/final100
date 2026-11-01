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
    #[serde(default)]
    pub notlar: String,
    /// Bu işletmenin mesaj varyantı (eski kayıtlarda boş; sonraki taramada atanır).
    #[serde(default)]
    pub varyant: Option<Varyant>,
}

/// Defterin özet sayaçları (arayüzde ve günlükte gösterilir).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ozet {
    pub toplam: usize,
    pub yeni: usize,
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
            let tamamla = eksik_varyant.then(|| varyant_sec(&self.dosya.musteriler));
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
        let varyant = varyant_sec(&self.dosya.musteriler);
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
            notlar: String::new(),
            varyant: Some(varyant),
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
        for kayit in &self.dosya.musteriler {
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
                Durum::Gonderildi | Durum::MesajHazir => ozet.gonderilen += 1,
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
            "ad;telefon;adres;kategori;site;site_linki;puan;durum;ilk_gorulme;son_gorulme;son_temas;kaynak;varyant\n",
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
                musteri.kaynak.as_str(),
                musteri.varyant.map(|v| v.kod()).unwrap_or(""),
            ];
            let satir: Vec<String> = alanlar.iter().map(|alan| csv_alan(alan)).collect();
            cikti.push_str(&satir.join(";"));
            cikti.push('\n');
        }
        cikti
    }
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
fn varyant_sec(kayitlar: &[Musteri]) -> Varyant {
    let a = kayitlar
        .iter()
        .filter(|kayit| kayit.varyant == Some(Varyant::A))
        .count();
    let b = kayitlar
        .iter()
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
        assert!(
            csv.lines().next().unwrap_or_default().ends_with(";varyant"),
            "başlıkta varyant sütunu olmalı"
        );
        let satir = csv.lines().nth(1).expect("veri satırı");
        assert!(satir.ends_with(";A") || satir.ends_with(";B"), "{satir}");
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
    fn bozuk_dosya_bos_defter_dondurur() {
        let defter = Defter::metinden("{bozuk json");
        assert_eq!(defter.ozet().toplam, 0);
        let defter = Defter::metinden("");
        assert_eq!(defter.ozet().toplam, 0);
    }
}
