//! Demo sitesi üreticisi.
//!
//! Uygulamanın "Demo Üret" akışı bu modülü kullanır: işletme kaydındaki gerçek
//! verileri (ad, adres, telefon, çalışma saatleri, konum, puan) sektöre özel
//! metinlerle birleştirip tek dosyalık, bağımsız bir tanıtım sayfası üretir.
//!
//! Tasarım kuralları:
//! - Dış istek yok: CSS/JS/fontlar sayfanın içinde (CDN, internet gerekmez).
//! - Tailwind gibi bir çalışma zamanı yok; ölçü ve boşluklar sabit ızgarada.
//! - Hareket; satır maskesi, kaydırma açılımı, parallax, akan şerit ve
//!   imleci izleyen ışıkla sınırlı — `prefers-reduced-motion` hepsini kapatır.
//! - İşletme hakkında uydurma iddia yok: puan/yorum yalnızca kayıtta varsa,
//!   fiyat ve yorumların örnek olduğu sayfada açıkça yazar.

/// Sayfayı üretmek için gereken her şey; `main.rs` hazırlayıp buraya verir.
pub struct Girdi {
    pub ad: String,
    pub adres: String,
    pub sektor: String,
    /// Sektörün demo ailesi: "menu", "kafe", "unlu", "urun", "galeri",
    /// "klinik", "otel", "egitim", "ofis", "servis", "eglence".
    pub tur: &'static str,
    /// Vurgu rengi (#rrggbb). Boşsa sektör rengi kullanılır.
    pub vurgu: String,
    /// Sayfa zemini ve metin rengi (#rrggbb): uygulamadaki renklendirme paneli.
    pub zemin: String,
    pub yazi: String,
    /// İşletmenin fotoğrafı: (mime, base64). Varsa hero sahnesinde kullanılır.
    pub foto: Option<(String, String)>,
    /// Fotoğrafın atfı (lisans gereği): altbilgide görünür.
    pub foto_atif: Option<String>,
    pub telefon: Option<String>,
    pub telefon_goster: String,
    /// Haritanın gömüleceği koordinat; yoksa harita bölümü çizilmez.
    pub konum: Option<(f64, f64)>,
    /// Haftalık program (JS dizisi); saat verisi yoksa `None`.
    pub program: Option<String>,
    pub saat_metni: String,
    pub saat_satirlari: Vec<(String, String)>,
    pub puan: Option<String>,
    pub yorum: Option<String>,
    pub karne: Option<String>,
    pub kaynak: String,
    pub gonderen: String,
    pub anahtar: String,
    pub font_css: &'static str,
}

struct Icerik {
    /// İkinci bölümün üst etiketi (başlığın kısa adı).
    etiket: &'static str,
    /// Manşet iki satıra bölünür; satırlar maskeden yukarı kayarak girer.
    manset: [&'static str; 2],
    alt: &'static str,
    /// Hakkında bölümünün başlığı.
    hikaye_baslik: &'static str,
    hikaye: [&'static str; 2],
    bolum: &'static str,
    bolum_alt: &'static str,
    /// (ad, açıklama, fiyat) — fiyatların örnek olduğu sayfada yazar.
    kalemler: [(&'static str, &'static str, &'static str); 4],
    neden: [(&'static str, &'static str); 3],
    cta: &'static str,
    akis: &'static str,
    yorumlar: [(&'static str, &'static str); 2],
}

fn icerik(tur: &str) -> Icerik {
    match tur {
        "menu" => Icerik {
            etiket: "MENÜ",
            manset: ["Ateşin ve sofranın", "ortak dili."],
            alt: "Günlük taze malzeme, meşe kömürü ve uzun sofralar. Mutfak her sabah yeniden kurulur.",
            hikaye_baslik: "Otuz yıldır aynı ocak.",
            hikaye: [
                "Ocağın başında geçen otuz yıl bize tek bir şeyi öğretti: iyi yemek acele etmez. Sebze sabah pazardan, et aynı gün kasaptan gelir; menü mevsime göre değişir.",
                "Sofraya oturduğunuzda garsonun ilk sorusu ne yiyeceğiniz değil, ne kadar kalacağınız. Acele ettirmiyoruz — burası hızlı yemek yeri değil.",
            ],
            bolum: "Menüden seçkiler",
            bolum_alt: "Mevsimlik menünün bir bölümü. Tam liste işletmede ve telefonda.",
            kalemler: [
                ("Günün çorbası", "Mevsim sebzeleri, tereyağı, taze nane", "₺120"),
                ("Adana kebap", "Zırhla kıyılmış kuzu, közlenmiş biber, sumaklı soğan", "₺320"),
                ("Kuzu tandır", "Sekiz saat kapalı fırında, kuyruk yağıyla", "₺420"),
                ("Fıstıklı kadayıf", "Günlük açılan kadayıf, Antep fıstığı, kaymak", "₺160"),
            ],
            neden: [
                ("Günlük tedarik", "Sebze ve et her sabah gelir; bir gün bile beklemez."),
                ("Odun ateşi", "Gaz değil meşe kömürü — kokusu tabağa siner."),
                ("Uzun sofra", "Kalabalık gruplar için ayrı salon; önceden ayırtılır."),
            ],
            cta: "Masa ayırt",
            akis: "Kebap · Meze · Rakı · Tatlı",
            yorumlar: [
                ("Tandır geldiğinde masa sustu. İki saat oturduk, kimse kaldırmadı.", "Akşam yemeği"),
                ("Rezervasyonu telefonla yaptık, saatinde masa hazırdı.", "Kalabalık grup"),
            ],
        },
        "kafe" => Icerik {
            etiket: "MENÜ",
            manset: ["Sabahın ilk fincanı,", "akşamın son sohbeti."],
            alt: "Üçüncü nesil çekirdek, haftalık kavurma. Sütü çevredeki üreticiden alıyoruz.",
            hikaye_baslik: "Haftada iki kez kavuruyoruz.",
            hikaye: [
                "Çekirdeği kendi mutfağımızda, haftada iki kez küçük partiler hâlinde kavuruyoruz. Demleme suyunun sertliğini ölçüyor, her yeni çekirdekte öğütmeyi yeniden ayarlıyoruz.",
                "Masa sizi bekletmez: priz var, wifi var, iki saat otursanız kimse bakmaz. Ama gürültü yok — konuşmak için geldiyseniz de doğru yerdesiniz.",
            ],
            bolum: "Menü",
            bolum_alt: "Çekirdekler mevsime göre değişir; barista önerir.",
            kalemler: [
                ("Filtre kahve", "Tek origin, V60, günün çekirdeği", "₺95"),
                ("Flat white", "Çift shot, mikro köpük, 160 ml", "₺110"),
                ("Cold brew", "18 saat soğuk demleme, buzla", "₺120"),
                ("Ev yapımı cheesecake", "Günlük, fırın tepsisiyle sınırlı", "₺145"),
            ],
            neden: [
                ("Kendi kavurmamız", "Çekirdek her hafta taze kavrulur, raf ömrü kısa tutulur."),
                ("Sessiz köşe", "Priz, wifi ve rahat oturma; uzun kalmak sorun değil."),
                ("Ev yapımı tatlı", "Tatlılar her sabah mutfakta hazırlanır, kalan ertesi güne çıkmaz."),
            ],
            cta: "Masa ayırt",
            akis: "Espresso · Filtre · Tatlı · Sohbet",
            yorumlar: [
                ("Filtre kahvesi şehirdeki en dengelisi; acı değil, meyvemsi.", "Sabah kahvesi"),
                ("Laptopla üç saat çalıştım, kimse rahatsız etmedi.", "Uzaktan çalışan"),
            ],
        },
        "unlu" => Icerik {
            etiket: "TEZGÂH",
            manset: ["Her sabah 05:00'te", "yanan ocak."],
            alt: "Ekşi mayalı ekmek, taze simit ve pastane tatlıları; hepsi aynı tezgâhta.",
            hikaye_baslik: "Maya 2011'den beri aynı kavanozda.",
            hikaye: [
                "Mayamız 2011'den beri aynı kavanozda. Un, su ve tuz; başka bir şey girmiyor. Hamur 36 saat dinleniyor, bu yüzden ekmek ertesi gün de taze kalıyor.",
                "Vitrinde ne varsa o gün üretildi. Kapanışa kalan ürünü ertesi güne çıkarmıyoruz — gün sonu indirimi bu yüzden var.",
            ],
            bolum: "Tezgâhtan",
            bolum_alt: "Günlük üretim; bazı ürünler öğleden sonra biter.",
            kalemler: [
                ("Ekşi mayalı somun", "36 saat fermentasyon, tam buğday katkılı", "₺85"),
                ("Simit", "Susamlı, taş fırın", "₺15"),
                ("Su böreği (porsiyon)", "Günlük açılmış yufka, peynirli", "₺95"),
                ("Doğum günü pastası", "Siparişe göre, 24 saat önceden", "₺650"),
            ],
            neden: [
                ("Ekşi maya", "Katkı maddesi yok; maya, un, su, tuz."),
                ("Günlük üretim", "Kalan ürün ertesi güne devredilmez."),
                ("Sipariş", "Pasta ve börek için 24 saat önce arayın."),
            ],
            cta: "Sipariş ver",
            akis: "Ekmek · Simit · Börek · Pasta",
            yorumlar: [
                ("Ekmeği iki gün sonra da kızartmadan yiyoruz.", "Haftalık ekmek"),
                ("Pastayı bir gün önceden söyledik, tam istediğimiz gibi geldi.", "Doğum günü"),
            ],
        },
        "urun" => Icerik {
            etiket: "SEÇKİ",
            manset: ["Seçilmiş ürün,", "dürüst fiyat."],
            alt: "Her sezon az sayıda parça. Denemeden almaya gerek yok; değişim otuz gün.",
            hikaye_baslik: "Vitrine giren her parça denenir.",
            hikaye: [
                "Vitrine koyduğumuz her parçayı önce kendimiz kullanıyoruz. Dikişi, kumaşı, yıkandıktan sonraki hâli tutmuyorsa rafa girmiyor.",
                "Beden konusunda tahmin yürütmüyoruz: ölçü tablosu kapının yanında, deneme kabini var. Almadan çıkmak da sorun değil.",
            ],
            bolum: "Sezon seçkisi",
            bolum_alt: "Beden ve renk stokları mağazaya göre değişir.",
            kalemler: [
                ("Gömlek", "Pamuk poplin, regular kalıp, altı renk", "₺890"),
                ("Pantolon", "Yüksek bel, %2 elastan, dört beden", "₺1.290"),
                ("Triko", "Merinos karışım, ince örgü", "₺1.150"),
                ("Aksesuar", "Kemer, atkı, çanta", "₺390"),
            ],
            neden: [
                ("Sınırlı koleksiyon", "Her bedenden az adet; aynı parçayı herkeste görmezsiniz."),
                ("Otuz gün değişim", "Etiketi sökülmemiş ürün koşulsuz değişir."),
                ("Kombin yardımı", "Randevusuz; satıcı değil, danışman gibi çalışıyoruz."),
            ],
            cta: "Mağazayı ara",
            akis: "Giyim · Aksesuar · Beden · Değişim",
            yorumlar: [
                ("Bedenimi bilmeden gittim, on dakikada doğru kalıbı bulduk.", "İlk alışveriş"),
                ("Aldığım gömleği bir beden büyüğüyle sorunsuz değiştirdiler.", "Değişim"),
            ],
        },
        "galeri" => Icerik {
            etiket: "HİZMETLER",
            manset: ["Aynaya bakmadan", "çıkmıyorsunuz."],
            alt: "Kesim, renk ve bakım saçınızın yapısına göre planlanır; şablon yok.",
            hikaye_baslik: "İlk on dakika makas açılmıyor.",
            hikaye: [
                "İlk on dakika makas açılmıyor: saçın ne kadar yıprandığını, ne sıklıkla şekil verdiğinizi, sabah ne kadar vaktiniz olduğunu konuşuyoruz. Plan buna göre çıkıyor.",
                "Boyada tek marka kullanmıyoruz; saçın direncine göre seçiyoruz. Kullandığımız ürünü sormak ayıp değil — hepsini söylüyoruz.",
            ],
            bolum: "Hizmetler",
            bolum_alt: "Fiyatlar saç uzunluğuna göre değişebilir; telefonda netleşir.",
            kalemler: [
                ("Saç kesimi", "Yıkama ve fön dahil, 45 dakika", "₺450"),
                ("Boya & röfle", "Saç analizine göre teknik, 2–3 saat", "₺1.800"),
                ("Keratin bakım", "Yıpranmış saç için, 2 saat", "₺2.400"),
                ("Gelin saçı", "Deneme seansı dahil", "₺3.500"),
            ],
            neden: [
                ("Konsültasyon", "İlk on dakika sadece dinliyoruz."),
                ("Randevu disiplini", "Saatler 15 dakikalık aralıklarla; bekleme yok."),
                ("Şeffaf ürün", "Hangi marka, hangi oran — sorduğunuzda söylüyoruz."),
            ],
            cta: "Randevu al",
            akis: "Kesim · Renk · Bakım · Gelin",
            yorumlar: [
                ("Yıllardır boya yaptırıyorum, ilk kez saçım yıpranmadı.", "Renk"),
                ("Randevu saatinde başladı, söylenen sürede bitti.", "Kesim"),
            ],
        },
        "klinik" => Icerik {
            etiket: "TEDAVİLER",
            manset: ["Tedavi planı,", "ilk muayenede."],
            alt: "Dijital görüntüleme, tek kullanımlık setler ve yazılı tedavi planı.",
            hikaye_baslik: "Plan, ilk muayenede çıkar.",
            hikaye: [
                "İlk muayenede ağzınızın tamamını görüyoruz: röntgen ve ağız içi tarama ile. Sonra tek tek ne yapılacağını, kaç seansta biteceğini ve ne tutacağını yazılı veriyoruz.",
                "Ağrınız varsa aynı gün bakıyoruz. Tedaviye başlamadan önce alternatifleri de anlatıyoruz — en pahalı seçenek her zaman doğru seçenek değil.",
            ],
            bolum: "Tedaviler",
            bolum_alt: "Fiyatlar örnektir; kesin plan muayeneden sonra çıkar.",
            kalemler: [
                ("İlk muayene & röntgen", "Panoramik film ve ağız içi tarama dahil", "Ücretsiz"),
                ("Diş temizliği", "Ultrasonik temizlik, 30 dakika", "₺1.200"),
                ("Beyazlatma", "Ofis tipi, tek seans", "₺4.500"),
                ("İmplant", "Tek diş, kemik grefti hariç", "₺18.000"),
            ],
            neden: [
                ("Sterilizasyon", "Her set tek kullanımlık; otoklav kaydı istenebilir."),
                ("Dijital ölçü", "Ağız içi tarayıcı — ölçü macunu yok."),
                ("Yazılı plan", "Seans sayısı ve tutar, başlamadan önce yazılı."),
            ],
            cta: "Randevu al",
            akis: "Muayene · Temizlik · İmplant · Estetik",
            yorumlar: [
                ("Ne yapılacağını ve ne ödeyeceğimi baştan yazılı gördüm.", "İmplant"),
                ("Diş hekimi korkum vardı; ilk seansta bunu konuştuk.", "Tedavi"),
            ],
        },
        "otel" => Icerik {
            etiket: "ODALAR",
            manset: ["Şehirde ama", "gürültüden uzak."],
            alt: "On iki oda, kahvaltı dahil. Her oda farklı; hiçbiri standart değil.",
            hikaye_baslik: "On iki oda, on iki farklı plan.",
            hikaye: [
                "Bina 1920'lerden; odaları tek tek elden geçirdik. Yüksek tavan, ahşap zemin ve sokağa bakan pencereler kaldı, gerisi yenilendi.",
                "Kahvaltı açık büfe değil, masaya geliyor: yerel peynir, zeytin, günlük ekmek. Erken çıkmanız gerekiyorsa bir gece önce söyleyin, paket yaparız.",
            ],
            bolum: "Odalar",
            bolum_alt: "Fiyatlar sezona göre değişir; kesin fiyat rezervasyonda.",
            kalemler: [
                ("Standart oda", "18 m², çift kişilik, kahvaltı dahil", "₺2.400"),
                ("Deluxe oda", "26 m², balkon, kahvaltı dahil", "₺3.200"),
                ("Suit", "38 m², oturma odası ayrı", "₺4.800"),
                ("Kahvaltı (misafir dışı)", "08:00–10:30, açık büfe", "₺350"),
            ],
            neden: [
                ("Kahvaltı dahil", "Yerel ürünlerle, 08:00–10:30 arası."),
                ("Yürüme mesafesi", "Tarihi merkez ve sahil on dakika."),
                ("Esnek iptal", "Girişten 48 saat öncesine kadar ücretsiz."),
            ],
            cta: "Rezervasyon",
            akis: "Oda · Kahvaltı · Konum · İptal",
            yorumlar: [
                ("Odalar fotoğraftakinden büyük; sessizlik en büyük artısı.", "Hafta sonu"),
                ("Kahvaltı masaya geldi, otel büfesi gibi değil.", "Konaklama"),
            ],
        },
        "egitim" => Icerik {
            etiket: "PROGRAMLAR",
            manset: ["Küçük sınıf,", "çok konuşma."],
            alt: "Sekiz kişilik gruplar, ücretsiz seviye sınavı ve haftada iki konuşma seansı.",
            hikaye_baslik: "Sekiz kişilik sınıflar.",
            hikaye: [
                "Sınıflar en fazla sekiz kişi. Seviye sınavı kayıttan önce yapılır; kimse yanlış gruba başlamaz. Ay sonunda ilerleme yazılı olarak paylaşılır.",
                "Dilbilgisi tek başına konuşmayı getirmiyor; bu yüzden haftada iki seans yalnızca konuşmaya ayrıldı. Hata yapmak serbest, susmak değil.",
            ],
            bolum: "Programlar",
            bolum_alt: "Gruplar ayda bir açılır; kontenjan sekiz kişi.",
            kalemler: [
                ("Genel İngilizce", "A1–C1, haftada 2 gün, 8 hafta", "₺3.200 / ay"),
                ("Sınav hazırlığı", "IELTS / YDS odaklı, 10 hafta", "₺4.400 / ay"),
                ("Birebir ders", "Öğretmenle teke tek, 60 dakika", "₺700 / saat"),
                ("Çocuk grubu", "8–12 yaş, haftada 2 gün", "₺2.600 / ay"),
            ],
            neden: [
                ("Sekiz kişi", "Sınıf mevcudu sekizle sınırlı."),
                ("Konuşma odağı", "Haftada iki seans yalnızca konuşma."),
                ("Ücretsiz seviye testi", "Kayıttan önce, sonuç yazılı."),
            ],
            cta: "Seviye testi",
            akis: "İngilizce · Sınav · Konuşma · Çocuk",
            yorumlar: [
                ("Altı ay önce konuşamıyordum, şimdi toplantıda konuşuyorum.", "Genel İngilizce"),
                ("Çocuk grubunda hocayı sevdi, devam etmek istiyor.", "Çocuk grubu"),
            ],
        },
        "ofis" => Icerik {
            etiket: "HİZMETLER",
            manset: ["İlk görüşmede", "net bir yol haritası."],
            alt: "Dosyanızı dinliyor, süreci ve maliyeti yazılı olarak önünüze koyuyoruz.",
            hikaye_baslik: "Belirsizliği yazıya döküyoruz.",
            hikaye: [
                "Danışmanlıkta en büyük sorun belirsizlik: ne olacağını, ne kadar süreceğini ve ne tutacağını bilmemek. İlk görüşmede bunların hepsini kâğıda döküyoruz.",
                "Dosyanızı bir ekip değil, tek bir sorumlu yürütür. Aradığınızda kimi aradığınızı bilirsiniz; her seferinde baştan anlatmanız gerekmez.",
            ],
            bolum: "Hizmetler",
            bolum_alt: "Kapsam ve ücret, ilk görüşmeden sonra yazılı teklifle netleşir.",
            kalemler: [
                ("İlk görüşme", "30 dakika, dosya değerlendirmesi", "Ücretsiz"),
                ("Danışmanlık", "Saatlik, yazılı özet dahil", "₺2.500"),
                ("Sözleşme incelemesi", "Risk notlarıyla birlikte", "₺6.000"),
                ("Aylık takip", "Süreç ve yazışma takibi", "₺9.000"),
            ],
            neden: [
                ("Yazılı kapsam", "Ne yapılacağı ve süresi baştan yazılır."),
                ("Gizlilik", "Dosyalar yalnızca sorumlu uzmanda; üçüncü tarafla paylaşılmaz."),
                ("Aynı gün dönüş", "Mesaj ve e-postaya aynı iş günü içinde dönülür."),
            ],
            cta: "Görüşme planla",
            akis: "Danışmanlık · Sözleşme · Takip · Gizlilik",
            yorumlar: [
                ("Ne yapılacağını ve ne ödeyeceğimi ilk görüşmede yazılı aldım.", "Danışmanlık"),
                ("Süreç boyunca aynı kişiyle konuştum, her seferinde baştan anlatmadım.", "Dosya takibi"),
            ],
        },
        "servis" => Icerik {
            etiket: "HİZMETLER",
            manset: ["Yolda kaldıysanız,", "ortalama 25 dakika."],
            alt: "7/24 çekici, akü takviye ve lastik. Konumunuzu gönderin, en yakın aracı yönlendirelim.",
            hikaye_baslik: "Telefonda söylenen fiyat değişmez.",
            hikaye: [
                "Çağrı merkezi değil, doğrudan sürücüyle konuşuyorsunuz. Konumunuzu attığınızda en yakın araç yola çıkıyor ve tahmini varış süresini mesajla alıyorsunuz.",
                "Fiyat telefonda söylenir, iş bitince değişmez. Aracınızı nereye götürdüğümüzü ve teslim saatini de yine mesajla bildiriyoruz.",
            ],
            bolum: "Hizmetler",
            bolum_alt: "Fiyatlar şehir içi standart tarifedir; mesafe değişirse baştan söylenir.",
            kalemler: [
                ("Şehir içi çekici", "İlk 15 km dahil", "₺1.500"),
                ("Akü takviye", "Yerinde, 15 dakika", "₺800"),
                ("Lastik değişimi", "Yedek lastik varsa", "₺700"),
                ("Şehirler arası", "Kilometre başı, önceden hesaplanır", "₺45 / km"),
            ],
            neden: [
                ("7/24", "Gece, hafta sonu, resmî tatil fark etmez."),
                ("Sabit fiyat", "Telefonda söylenen tutar değişmez."),
                ("Takip", "Aracın nereye gittiğini mesajla bildiririz."),
            ],
            cta: "Hemen ara",
            akis: "Çekici · Akü · Lastik · 7/24",
            yorumlar: [
                ("Gece 02:00'de aradım, 20 dakikada geldiler.", "Gece çağrısı"),
                ("Telefonda söylenen fiyatın dışında ek ücret istemediler.", "Şehir içi çekici"),
            ],
        },
        "eglence" => Icerik {
            etiket: "PAKETLER",
            manset: ["Salon boş,", "sahne sizin."],
            alt: "Düğün, nişan ve kurumsal etkinlikler için 400 kişilik salon; ses ve ışık dahil.",
            hikaye_baslik: "Aynı gün tek etkinlik.",
            hikaye: [
                "Mekânı tek etkinliğe açıyoruz: aynı gün iki düğün, iki telaş olmaz. Kurulum sabah başlar, siz gelmeden sahne hazır olur.",
                "Mutfak bizim; menüyü birlikte tadıyoruz. Paket fiyat sabit — masa sayısı, ses, ışık ve servis dahil, sonradan ek kalem çıkmaz.",
            ],
            bolum: "Paketler",
            bolum_alt: "Kişi sayısı ve tarihe göre değişir; kesin teklif görüşmede.",
            kalemler: [
                ("Düğün paketi", "400 kişi, menü, ses-ışık, servis", "₺120.000"),
                ("Nişan paketi", "200 kişi, kokteyl ve müzik", "₺70.000"),
                ("Kurumsal toplantı", "Günlük kiralama, teknik ekip dahil", "₺25.000"),
                ("Ses & ışık", "Tek başına kiralama, teknik ekip dahil", "₺12.000"),
            ],
            neden: [
                ("Sabit paket", "Sürpriz ek ücret yok; sözleşmede her kalem yazılı."),
                ("Otopark", "120 araçlık kapalı otopark, vale hizmeti."),
                ("Kendi mutfağı", "Menü tadımı etkinlikten önce yapılır."),
            ],
            cta: "Tarih sor",
            akis: "Düğün · Nişan · Toplantı · Sahne",
            yorumlar: [
                ("Sözleşmede yazan tutar dışında tek kuruş ödemedik.", "Düğün"),
                ("Kurumsal toplantıda teknik ekip sorunsuz çalıştı.", "Şirket toplantısı"),
            ],
        },
        _ => Icerik {
            etiket: "HİZMETLER",
            manset: ["İşinizi", "anlatan bir sayfa."],
            alt: "Hizmetler, konum ve iletişim tek sayfada; mobilde hızlı açılır.",
            hikaye_baslik: "İşinizi anlatan bir sayfa.",
            hikaye: [
                "Bu sayfa işletmenizin dijital vitrinidir: ne yaptığınızı, nerede olduğunuzu ve size nasıl ulaşılacağını tek ekranda anlatır.",
                "İçerik, fotoğraf ve fiyatlar sizin bilgilerinizle değiştirilir; bu hâliyle bir taslaktır.",
            ],
            bolum: "Hizmetler",
            bolum_alt: "Liste, işletmenizin gerçek hizmetleriyle değiştirilecek.",
            kalemler: [
                ("Hizmet", "Kısa açıklama", "₺—"),
                ("Hizmet", "Kısa açıklama", "₺—"),
                ("Hizmet", "Kısa açıklama", "₺—"),
                ("Hizmet", "Kısa açıklama", "₺—"),
            ],
            neden: [
                ("Hızlı açılır", "Tek dosya, dış bağımlılık yok."),
                ("Mobil uyumlu", "Telefonda tek elle kullanılır."),
                ("Tek dokunuş", "Arama ve yol tarifi doğrudan."),
            ],
            cta: "İletişime geç",
            akis: "Hizmet · Konum · İletişim",
            yorumlar: [
                ("Bu alan müşteri yorumlarıyla doldurulacak.", "Örnek yorum"),
                ("Bu alan müşteri yorumlarıyla doldurulacak.", "Örnek yorum"),
            ],
        },
    }
}

/// Sayfanın tamamı tek dosyada: harici CSS/JS/font isteği yok.
const CSS: &str = r#"
*,*::before,*::after{box-sizing:border-box}
html{-webkit-text-size-adjust:100%;scroll-behavior:smooth}
body{margin:0;background:var(--bg);color:var(--ink);font-family:var(--f-govde);font-size:16px;line-height:1.62;-webkit-font-smoothing:antialiased;overflow-x:hidden}
img,iframe{max-width:100%;display:block}
a{color:inherit;text-decoration:none}
h1,h2,h3{font-family:var(--f-baslik);font-weight:700;letter-spacing:-.03em;margin:0}
:root{--dim:color-mix(in srgb,var(--ink) 62%,transparent);--cizgi:color-mix(in srgb,var(--ink) 12%,transparent);--kart:color-mix(in srgb,var(--ink) 4%,transparent);--accent-metin:color-mix(in srgb,var(--accent) 76%,var(--ink))}
.kabuk{max-width:1180px;margin:0 auto;padding:0 26px}
.ince{font-size:11.5px;letter-spacing:.22em;text-transform:uppercase;color:var(--dim)}
/* --- tanıtım şeridi --- */
.serit{background:color-mix(in srgb,var(--accent) 14%,var(--bg));color:var(--accent-metin);text-align:center;font-size:11px;letter-spacing:.16em;text-transform:uppercase;padding:9px 18px;border-bottom:1px solid var(--cizgi)}
/* --- üst menü --- */
.ust{position:sticky;top:0;z-index:60;background:color-mix(in srgb,var(--bg) 84%,transparent);backdrop-filter:blur(16px) saturate(1.2);border-bottom:1px solid var(--cizgi);transition:padding .35s ease,box-shadow .35s ease}
.ust-ic{max-width:1180px;margin:0 auto;padding:15px 26px;display:flex;align-items:center;gap:18px}
.ust.kucuk .ust-ic{padding:9px 26px}
.ilerleme{position:fixed;left:0;top:0;height:2px;width:0;background:var(--accent);z-index:80;transition:width .1s linear;box-shadow:0 0 12px var(--accent)}
.marka{display:flex;align-items:center;gap:11px;min-width:0;font-family:var(--f-baslik);font-weight:700;font-size:15.5px;letter-spacing:-.01em}
.monogram{width:36px;height:36px;border-radius:11px;background:var(--accent);color:var(--accent-karsi);display:grid;place-items:center;font-size:15px;flex:0 0 auto;box-shadow:0 8px 24px -10px var(--accent)}
.marka span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.menuler{display:flex;align-items:center;gap:26px;margin-left:auto;font-size:13.5px;color:var(--dim)}
.menuler a:hover{color:var(--ink)}
.ust-tel{display:inline-flex;align-items:center;gap:8px;border:1px solid var(--cizgi);border-radius:999px;padding:9px 17px;font-size:13px;font-weight:600;transition:border-color .25s,background .25s}
.ust-tel svg{width:15px;height:15px;flex:0 0 auto}
.ust-tel:hover{border-color:color-mix(in srgb,var(--accent) 60%,transparent);background:color-mix(in srgb,var(--accent) 10%,transparent)}
.mobil-dugme{display:none;margin-left:auto;background:none;border:1px solid var(--cizgi);color:var(--ink);border-radius:10px;padding:7px 11px;font-size:15px}
.mobil-menu{display:none;border-top:1px solid var(--cizgi);padding:14px 26px 18px;gap:14px;flex-direction:column;font-size:15px;color:var(--dim)}
.mobil-menu.acik{display:flex}
/* --- hero --- */
.hero{position:relative;overflow:hidden;padding:clamp(40px,6vw,84px) 0 clamp(44px,6vw,76px)}
.hero-ic{display:grid;grid-template-columns:minmax(0,1.08fr) minmax(0,.92fr);gap:clamp(30px,5vw,72px);align-items:center;min-height:min(68vh,640px)}
.heroi{position:relative;z-index:2}
.rozet-cip{display:inline-flex;align-items:center;gap:9px;border:1px solid color-mix(in srgb,var(--accent) 34%,transparent);border-radius:999px;padding:6px 14px;font-size:11px;letter-spacing:.19em;text-transform:uppercase;color:var(--accent-metin);background:color-mix(in srgb,var(--accent) 9%,transparent)}
.rozet-cip b{width:5px;height:5px;border-radius:50%;background:var(--accent);box-shadow:0 0 12px var(--accent)}
.manset{font-size:clamp(38px,5.7vw,78px);line-height:1.02;letter-spacing:-.042em;margin:22px 0 0}
.manset span{display:block;overflow:hidden;padding-bottom:.08em}
.manset i{display:block;font-style:normal;transform:translateY(112%);animation:satir .95s cubic-bezier(.19,.78,.18,1) forwards;animation-delay:calc(var(--i) * 110ms + 80ms)}
.hero-alt{margin:22px 0 0;max-width:52ch;color:var(--dim);font-size:clamp(14.5px,1.35vw,17px);line-height:1.68}
.hero-butonlar{display:flex;flex-wrap:wrap;gap:11px;margin-top:30px}
.buton{display:inline-flex;align-items:center;gap:9px;border-radius:999px;padding:14px 24px;font-size:14px;font-weight:650;letter-spacing:.01em;transition:transform .25s cubic-bezier(.2,.8,.2,1),box-shadow .25s,background .25s,border-color .25s}
.buton svg{width:15px;height:15px}
.buton-dolu{background:var(--accent);color:var(--accent-karsi);box-shadow:0 16px 40px -18px var(--accent)}
.buton-dolu:hover{transform:translateY(-2px);box-shadow:0 22px 48px -18px var(--accent)}
.buton-cizgi{border:1px solid var(--cizgi);color:var(--ink)}
.buton-cizgi:hover{border-color:color-mix(in srgb,var(--accent) 55%,transparent);background:color-mix(in srgb,var(--accent) 8%,transparent)}
.durum{margin-top:26px;display:inline-flex;align-items:center;gap:10px;font-size:13.5px;color:var(--dim)}
.durum b{font-weight:650}
.nokta-canli{width:7px;height:7px;border-radius:50%;background:#4ade80;box-shadow:0 0 0 0 rgba(74,222,128,.6);animation:nabiz 2.4s infinite}
.nokta-canli.kapali{background:color-mix(in srgb,var(--ink) 38%,transparent);animation:none}
.durum-metin{color:color-mix(in srgb,var(--ink) 62%,transparent)}
.durum-metin.acik{color:#6ee7a8}
.durum-metin.yakin{color:#fbbf24}
.tema-acik .durum-metin.acik{color:#1c7a4a}
.tema-acik .durum-metin.yakin{color:#9a5b06}
.tema-acik .nokta-canli.kapali{background:color-mix(in srgb,var(--ink) 30%,transparent)}
.nokta-canli.yakin{background:#fbbf24}
/* --- sahne (fotoğrafsız kompozisyon) --- */
.sahne{position:relative;aspect-ratio:4/5;border-radius:26px;overflow:hidden;border:1px solid color-mix(in srgb,var(--accent) 22%,transparent);background:radial-gradient(120% 95% at 78% 6%,color-mix(in srgb,var(--accent) 34%,transparent),transparent 62%),radial-gradient(95% 75% at 8% 96%,color-mix(in srgb,var(--accent) 16%,transparent),transparent 66%),linear-gradient(158deg,color-mix(in srgb,var(--ink) 7%,var(--bg)),var(--bg))}
.halkalar{position:absolute;inset:-38%;background:repeating-radial-gradient(circle at 58% 44%,transparent 0 22px,color-mix(in srgb,var(--ink) 7%,transparent) 22px 23px);animation:don 76s linear infinite}
.sahne-foto{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;object-position:50% 42%;filter:grayscale(.18) contrast(1.06) brightness(.9) saturate(.96);transform:scale(1.03);transition:transform 1.2s cubic-bezier(.2,.8,.2,1)}
.sahne:hover .sahne-foto{transform:scale(1.07)}
.sahne-perde{position:absolute;inset:0;background:linear-gradient(180deg,color-mix(in srgb,var(--accent) 34%,transparent),transparent 46%,color-mix(in srgb,var(--bg) 55%,transparent));mix-blend-mode:soft-light}
.sahne-koyu{position:absolute;inset:0;background:radial-gradient(115% 85% at 52% 42%,transparent 28%,color-mix(in srgb,var(--bg) 74%,transparent) 100%)}
.monogram-buyuk{position:absolute;left:50%;top:47%;transform:translate(-50%,-50%);font-family:var(--f-baslik);font-weight:700;font-size:min(26vw,260px);line-height:.8;color:transparent;-webkit-text-stroke:1px color-mix(in srgb,var(--accent) 50%,transparent);opacity:.55;user-select:none}
.sahne-alt{position:absolute;left:24px;right:24px;bottom:22px;display:flex;align-items:flex-end;justify-content:space-between;gap:14px;font-size:10.5px;letter-spacing:.24em;text-transform:uppercase;color:color-mix(in srgb,var(--ink) 45%,transparent)}
/* Fotoğraf üzerindeki yazı her temada açık renk kalır. */
.sahne.foto .sahne-alt{color:rgba(255,255,255,.68);text-shadow:0 1px 10px rgba(0,0,0,.5)}
.sahne.foto .sahne-koyu{background:linear-gradient(180deg,transparent 42%,rgba(0,0,0,.5)),radial-gradient(115% 85% at 52% 42%,transparent 30%,color-mix(in srgb,var(--bg) 62%,transparent) 100%)}
.rozet{position:absolute;border-radius:15px;padding:12px 15px;background:color-mix(in srgb,var(--bg) 78%,transparent);backdrop-filter:blur(12px);border:1px solid var(--cizgi);box-shadow:0 18px 40px -24px color-mix(in srgb,var(--bg) 80%,#000)}
.rozet small{display:block;font-size:9.5px;letter-spacing:.2em;text-transform:uppercase;color:var(--dim);margin-bottom:3px}
.rozet b{font-family:var(--f-baslik);font-size:15px;letter-spacing:-.01em;font-weight:650}
.rozet-puan{display:flex;align-items:baseline;gap:7px}
.rozet-puan small{display:inline;margin:0;font-size:10.5px;letter-spacing:.08em;text-transform:none}
.rozet-a{left:20px;top:20px;animation:yuz 9s ease-in-out infinite;min-width:132px}
.rozet-b{right:20px;bottom:70px;animation:yuz 11s ease-in-out infinite reverse;min-width:158px}
.yildizlar{display:flex;gap:2px;margin-bottom:5px}
.yildizlar svg{width:11px;height:11px;color:var(--accent)}
/* --- istatistik şeridi --- */
.istatistik{border-top:1px solid var(--cizgi);border-bottom:1px solid var(--cizgi);margin-top:clamp(26px,4vw,44px)}
.istatistik-ic{display:grid;grid-template-columns:repeat(4,1fr);max-width:1180px;margin:0 auto;padding:0 26px}
.ist{background:var(--bg);padding:20px 22px;border-left:1px solid var(--cizgi)}
.ist:first-child{padding-left:0;border-left:0}
.ist small{display:block;font-size:10px;letter-spacing:.2em;text-transform:uppercase;color:var(--dim);margin-bottom:6px}
.ist b{font-family:var(--f-baslik);font-size:15px;font-weight:650;letter-spacing:-.01em}
/* --- akan şerit --- */
.akis{overflow:hidden;border-bottom:1px solid var(--cizgi);background:color-mix(in srgb,var(--accent) 6%,transparent);padding:15px 0}
.akis-ic{display:flex;width:max-content;animation:kay 38s linear infinite}
.akis-ic span{font-family:var(--f-baslik);font-size:13px;letter-spacing:.24em;text-transform:uppercase;color:var(--accent-metin);padding:0 22px;white-space:nowrap;display:inline-flex;align-items:center;gap:22px}
.akis-ic span::after{content:"";width:4px;height:4px;border-radius:50%;background:color-mix(in srgb,var(--accent) 60%,transparent)}
/* --- bölümler --- */
.bolum{padding:clamp(40px,4.8vw,66px) 0}
.bolum-ust{display:flex;align-items:baseline;gap:14px;padding-top:16px;border-top:1px solid var(--cizgi);margin-bottom:clamp(26px,4vw,46px)}
.bolum-ust .no{font-family:var(--f-baslik);font-size:11.5px;letter-spacing:.2em;color:var(--accent)}
.bolum h2{font-size:clamp(26px,3.8vw,44px);line-height:1.06;max-width:22ch}
.hikaye{display:grid;grid-template-columns:minmax(0,.9fr) minmax(0,1.1fr);gap:clamp(24px,5vw,64px);align-items:start}
.hikaye-buyuk{font-size:clamp(18px,2.1vw,24px);line-height:1.52;color:var(--ink);letter-spacing:-.015em;margin:0}
.hikaye p{color:var(--dim);font-size:15px;margin:18px 0 0;max-width:60ch}
.ozet-kutu{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;background:var(--cizgi);border:1px solid var(--cizgi);border-radius:16px;overflow:hidden;margin-top:clamp(28px,4vw,44px)}
.ozet{background:linear-gradient(180deg,rgba(255,255,255,.03),transparent);padding:20px 18px}
.ozet b{display:block;font-family:var(--f-baslik);font-size:13px;letter-spacing:.01em;margin-bottom:5px}
.ozet span{font-size:13px;color:var(--dim);line-height:1.5;display:block}
/* --- menü / hizmet listesi --- */
.liste{list-style:none;margin:0;padding:0;border-top:1px solid var(--cizgi)}
.satir{padding:20px 0;border-bottom:1px solid var(--cizgi);transition:padding-left .3s ease,background .3s ease}
.satir:hover{padding-left:14px;background:linear-gradient(90deg,color-mix(in srgb,var(--accent) 7%,transparent),transparent 70%)}
.satir-ust{display:flex;align-items:baseline}
.satir h3{font-size:clamp(16px,1.6vw,19px);font-weight:650;letter-spacing:-.02em}
.nokta{flex:1;border-bottom:1px dotted color-mix(in srgb,var(--ink) 30%,transparent);margin:0 14px 5px;min-width:18px}
.fiyat{font-family:var(--f-baslik);color:var(--accent);font-weight:650;font-size:clamp(14px,1.5vw,17px);font-variant-numeric:tabular-nums;white-space:nowrap}
.satir p{margin:5px 0 0;color:var(--dim);font-size:14px;max-width:64ch}
.liste-alt{margin-top:16px;font-size:12px;color:color-mix(in srgb,var(--dim) 80%,transparent)}
/* --- neden biz --- */
.uc{display:grid;grid-template-columns:repeat(3,1fr);gap:18px;margin-top:8px}
.kart{border:1px solid var(--cizgi);border-radius:18px;padding:26px 22px;background:linear-gradient(180deg,color-mix(in srgb,var(--ink) 4%,transparent),transparent);transition:transform .35s cubic-bezier(.2,.8,.2,1),border-color .35s;position:relative;overflow:hidden}
.kart:hover{transform:translateY(-4px);border-color:color-mix(in srgb,var(--accent) 45%,transparent)}
.kart .sira{font-family:var(--f-baslik);font-size:11px;letter-spacing:.2em;color:var(--accent);display:block;margin-bottom:16px}
.kart h3{font-size:17px;font-weight:650;margin-bottom:9px}
.kart p{margin:0;color:var(--dim);font-size:14px;line-height:1.6}
/* --- yorumlar --- */
.yorumlar{display:grid;grid-template-columns:repeat(2,1fr);gap:18px}
.alinti{margin:0;border:1px solid var(--cizgi);border-radius:18px;padding:26px 24px;background:var(--kart);position:relative}
.alinti::before{content:"”";position:absolute;right:20px;top:4px;font-family:var(--f-baslik);font-size:70px;line-height:1;color:color-mix(in srgb,var(--accent) 20%,transparent)}
.alinti p{margin:0 0 16px;font-size:15.5px;line-height:1.6;color:var(--ink);position:relative;z-index:1}
.alinti footer{display:flex;align-items:center;gap:10px;font-size:12.5px;color:var(--dim)}
.alinti footer b{width:26px;height:26px;border-radius:8px;display:grid;place-items:center;background:color-mix(in srgb,var(--accent) 14%,transparent);color:var(--accent-metin);font-family:var(--f-baslik);font-size:11px;letter-spacing:.04em}
.ornek-not{font-size:11px;letter-spacing:.12em;text-transform:uppercase;color:color-mix(in srgb,var(--dim) 70%,transparent);margin-top:14px}
/* --- saatler & iletişim --- */
.iletisim{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:clamp(22px,4vw,44px);align-items:start}
.saatler{border:1px solid var(--cizgi);border-radius:18px;overflow:hidden}
.saat-satir{display:flex;justify-content:space-between;gap:16px;padding:12px 18px;font-size:14px;border-bottom:1px solid var(--cizgi);color:var(--dim)}
.saat-satir:last-child{border-bottom:0}
.saat-satir.bugun{background:color-mix(in srgb,var(--accent) 9%,transparent);color:var(--ink);font-weight:600}
.saat-satir.bugun span:first-child::before{content:"";display:inline-block;width:6px;height:6px;border-radius:50%;background:var(--accent);margin-right:9px;vertical-align:middle}
.saat-satir b{font-weight:inherit}
.ilet-kartlar{display:grid;gap:12px;margin-top:16px}
.ilet-kart{border:1px solid var(--cizgi);border-radius:14px;padding:16px 18px;display:flex;gap:13px;align-items:flex-start;transition:border-color .3s,background .3s}
.ilet-kart:hover{border-color:color-mix(in srgb,var(--accent) 45%,transparent);background:color-mix(in srgb,var(--accent) 6%,transparent)}
.ilet-kart svg{width:17px;height:17px;flex:0 0 auto;margin-top:3px;color:var(--accent)}
.ilet-kart small{display:block;font-size:10px;letter-spacing:.18em;text-transform:uppercase;color:var(--dim);margin-bottom:3px}
.ilet-kart b{font-weight:600;font-size:14px;line-height:1.5}
.harita{border:1px solid var(--cizgi);border-radius:18px;overflow:hidden;background:var(--bg);display:flex;flex-direction:column}
.harita iframe{width:100%;height:400px;border:0;filter:grayscale(1) invert(.92) contrast(.88) brightness(.95) hue-rotate(180deg)}
.harita-kart{margin-top:14px;border:1px solid color-mix(in srgb,var(--accent) 26%,transparent);background:linear-gradient(160deg,color-mix(in srgb,var(--accent) 12%,transparent),transparent);border-radius:18px;padding:22px 22px}
.harita-alt{display:flex;justify-content:space-between;align-items:center;padding:12px 16px;font-size:12.5px;color:var(--dim);border-top:1px solid var(--cizgi)}
.harita-alt a{color:var(--accent);font-weight:600}
.suresi-doldu{position:fixed;inset:0;z-index:999;background:var(--bg);color:var(--ink);display:grid;place-items:center;text-align:center;padding:24px;font-size:17px;line-height:1.6}
.suresi-doldu[hidden]{display:none}
/* --- kapanış bandı --- */
.bant{background:var(--accent);color:var(--accent-karsi);padding:clamp(46px,6vw,78px) 0;position:relative;overflow:hidden}
.bant::after{content:"";position:absolute;inset:0;background:repeating-linear-gradient(115deg,rgba(0,0,0,.045) 0 2px,transparent 2px 26px);pointer-events:none}
.bant-ic{position:relative;z-index:1;display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:26px}
.bant h2{font-size:clamp(26px,4vw,46px);line-height:1.05;max-width:20ch;color:var(--accent-karsi)}
.bant p{margin:10px 0 0;font-size:14.5px;color:color-mix(in srgb,var(--accent-karsi) 76%,transparent);max-width:44ch}
.bant .buton{background:var(--accent-karsi);color:var(--accent);box-shadow:none}
.bant .buton:hover{transform:translateY(-2px)}
/* --- altbilgi --- */
.altbilgi{border-top:1px solid var(--cizgi);padding:44px 0 40px;font-size:12.5px;color:var(--dim)}
.alt-ic{display:flex;flex-wrap:wrap;gap:22px;justify-content:space-between;align-items:flex-start}
.alt-marka{display:flex;align-items:center;gap:11px;font-family:var(--f-baslik);font-weight:650;color:var(--ink);font-size:14px}
.alt-baglar{display:flex;flex-wrap:wrap;gap:18px}
.alt-baglar a:hover{color:var(--ink)}
.alt-not{max-width:52ch;line-height:1.6;margin:14px 0 0}
.alt-not a{color:var(--accent)}
/* --- sabit mobil CTA --- */
.mobil-cta{display:none;position:fixed;left:12px;right:12px;bottom:12px;z-index:70;border-radius:16px;background:var(--accent);color:var(--accent-karsi);text-align:center;font-weight:700;padding:15px;box-shadow:0 16px 40px -14px rgba(0,0,0,.8)}
/* --- ışık & doku --- */
#fener{position:fixed;inset:0;z-index:3;pointer-events:none;opacity:0;transition:opacity .4s ease;background:radial-gradient(340px circle at var(--fx,50%) var(--fy,50%),color-mix(in srgb,var(--accent) 20%,transparent),transparent 68%);mix-blend-mode:screen}
@media (hover:hover) and (pointer:fine){#fener.acik{opacity:1}}
.gren{position:fixed;inset:0;z-index:2;pointer-events:none;opacity:.032;background-image:url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'><filter id='g'><feTurbulence type='fractalNoise' baseFrequency='.85' numOctaves='3'/></filter><rect width='160' height='160' filter='url(%23g)'/></svg>")}
/* --- hareket --- */
[data-rise]{opacity:0;transform:translateY(24px);transition:opacity .8s cubic-bezier(.19,.78,.18,1),transform .8s cubic-bezier(.19,.78,.18,1);transition-delay:calc(var(--g,0) * 1ms)}
[data-rise].in{opacity:1;transform:none}
@keyframes satir{to{transform:translateY(0)}}
@keyframes kay{to{transform:translateX(-50%)}}
@keyframes don{to{transform:rotate(360deg)}}
@keyframes yuz{0%,100%{transform:translateY(0)}50%{transform:translateY(-13px)}}
@keyframes nabiz{0%{box-shadow:0 0 0 0 rgba(74,222,128,.55)}70%{box-shadow:0 0 0 9px rgba(74,222,128,0)}100%{box-shadow:0 0 0 0 rgba(74,222,128,0)}}
/* --- duyarlı --- */
@media (max-width:1000px){
  .hero-ic{grid-template-columns:1fr;min-height:0;gap:34px}
  .sahne{aspect-ratio:16/10;max-height:340px;max-width:100%}
  .monogram-buyuk{font-size:min(34vw,190px)}
  .sahne-alt{display:none}
  .rozet-a{left:12px;top:12px;min-width:0;padding:10px 13px}
  .rozet-b{right:12px;bottom:12px;min-width:0;padding:10px 13px}
  .serit{font-size:10px;letter-spacing:.12em}
  .hikaye{grid-template-columns:1fr}
  .uc{grid-template-columns:1fr}
  .yorumlar{grid-template-columns:1fr}
  .iletisim{grid-template-columns:1fr}
  .istatistik-ic{grid-template-columns:repeat(2,1fr);padding:0 26px}
  .ist{padding:16px 14px}
  .ist:nth-child(odd){padding-left:0;border-left:0}
  .ist:nth-child(n+3){border-top:1px solid var(--cizgi)}
  .menuler,.ust-tel{display:none}
  .mobil-dugme{display:block}
  .mobil-cta{display:block}
  body{padding-bottom:74px}
  .harita iframe{height:320px}
}
@media (max-width:560px){
  .kabuk,.ust-ic{padding-left:18px;padding-right:18px}
  .ozet-kutu{grid-template-columns:1fr}
  .buton{padding:13px 20px}
}
.tema-acik .harita iframe{filter:grayscale(.25) contrast(1.02)}
.tema-acik #fener{mix-blend-mode:multiply;opacity:0}
.tema-acik .gren{opacity:.022}
@media (prefers-reduced-motion:reduce){
  *,*::before,*::after{animation-duration:.001ms!important;animation-iteration-count:1!important;transition-duration:.001ms!important}
  [data-rise]{opacity:1;transform:none}
  .manset i{transform:none}
  #fener,.gren{display:none}
}
"#;

/// Rengin algılanan parlaklığı; açık temada gölge/karartma yönü değişir.
pub fn parlak_mi(hex: &str) -> bool {
    let temiz = hex.trim_start_matches('#');
    if temiz.len() != 6 {
        return false;
    }
    let kanal = |bas: usize| u8::from_str_radix(&temiz[bas..bas + 2], 16).unwrap_or(0) as f32;
    let (r, g, b) = (kanal(0), kanal(2), kanal(4));
    0.2126 * r + 0.7152 * g + 0.0722 * b > 140.0
}

/// HTML özel karakterlerini kaçırır: işletme adı/not kullanıcı verisidir.
fn kacis(metin: &str) -> String {
    let mut sonuc = String::with_capacity(metin.len());
    for karakter in metin.chars() {
        match karakter {
            '&' => sonuc.push_str("&amp;"),
            '<' => sonuc.push_str("&lt;"),
            '>' => sonuc.push_str("&gt;"),
            '"' => sonuc.push_str("&quot;"),
            '\'' => sonuc.push_str("&#39;"),
            _ => sonuc.push(karakter),
        }
    }
    sonuc
}

/// Adresin son parçası genelde ilçe/il olur; istatistik şeridinde kullanılır.
fn ilce_metni(adres: &str) -> String {
    adres
        .split(',')
        .map(str::trim)
        .rfind(|parca| !parca.is_empty())
        .map(kacis)
        .unwrap_or_else(|| "Konum".to_string())
}

const YILDIZ: &str = r#"<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2.6l2.9 5.9 6.5.95-4.7 4.6 1.1 6.5L12 17.5l-5.8 3.05 1.1-6.5-4.7-4.6 6.5-.95z"/></svg>"#;
const TELEFON_IKON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M22 16.9v3a2 2 0 0 1-2.2 2 19.8 19.8 0 0 1-8.6-3.1 19.5 19.5 0 0 1-6-6A19.8 19.8 0 0 1 2.1 4.2 2 2 0 0 1 4.1 2h3a2 2 0 0 1 2 1.7c.1 1 .4 1.9.7 2.8a2 2 0 0 1-.5 2.1L8.1 9.9a16 16 0 0 0 6 6l1.3-1.3a2 2 0 0 1 2.1-.4c.9.3 1.8.6 2.8.7a2 2 0 0 1 1.7 2z"/></svg>"#;
const KONUM_IKON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0z"/><circle cx="12" cy="10" r="3"/></svg>"#;
fn harita_html(g: &Girdi, ad: &str) -> String {
    let Some((enlem, boylam)) = g.konum else {
        return String::new();
    };
    let bbox = format!(
        "{:.4},{:.4},{:.4},{:.4}",
        boylam - 0.004,
        enlem - 0.003,
        boylam + 0.004,
        enlem + 0.003
    );
    let gomulu = format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={bbox}&layer=mapnik&marker={enlem:.5},{boylam:.5}"
    );
    let ac = format!(
        "https://www.openstreetmap.org/?mlat={enlem:.5}&mlon={boylam:.5}#map=18/{enlem:.5}/{boylam:.5}"
    );
    format!(
        r##"<div class="harita">
<iframe title="{ad} konumu" loading="lazy" src="{gomulu}"></iframe>
<div class="harita-alt"><span>Konum</span><a href="{ac}" target="_blank" rel="noopener">Haritada aç ↗</a></div>
</div>"##
    )
}

fn saatler_html(g: &Girdi) -> String {
    if g.saat_satirlari.is_empty() {
        return format!(
            r##"<div class="saatler"><div class="saat-satir"><span>Çalışma saatleri</span><b>{}</b></div></div>"##,
            if g.saat_metni.is_empty() {
                "Telefonla öğrenin".to_string()
            } else {
                kacis(&g.saat_metni)
            }
        );
    }
    let mut satirlar = String::new();
    for (sira, (gun, saat)) in g.saat_satirlari.iter().enumerate() {
        satirlar.push_str(&format!(
            r##"<div class="saat-satir" data-gun="{sira}"><span>{}</span><b>{}</b></div>"##,
            kacis(gun),
            kacis(saat)
        ));
    }
    format!(r##"<div class="saatler">{satirlar}</div>"##)
}

fn istatistik_html(g: &Girdi, icerik: &Icerik) -> String {
    let mut kalemler = vec![
        (
            "Bugün".to_string(),
            r#"<b id="ist-bugun">—</b>"#.to_string(),
        ),
        (
            "Bölge".to_string(),
            format!("<b>{}</b>", ilce_metni(&g.adres)),
        ),
    ];
    match (&g.puan, &g.yorum) {
        (Some(puan), Some(yorum)) => {
            kalemler.push(("Google".to_string(), format!("<b>{puan} ★</b> · {yorum}")))
        }
        (Some(puan), None) => kalemler.push(("Google".to_string(), format!("<b>{puan} ★</b>"))),
        _ => kalemler.push((
            "Bölüm".to_string(),
            format!("<b>{}</b>", kacis(icerik.bolum)),
        )),
    }
    kalemler.push((
        "İletişim".to_string(),
        format!(
            "<b>{}</b>",
            if g.telefon.is_some() {
                kacis(&g.telefon_goster)
            } else {
                "Telefonla".to_string()
            }
        ),
    ));
    let mut govde = String::new();
    for (etiket, deger) in kalemler {
        govde.push_str(&format!(
            r##"<div class="ist"><small>{etiket}</small>{deger}</div>"##
        ));
    }
    format!(r##"<div class="istatistik"><div class="istatistik-ic">{govde}</div></div>"##)
}

/// Demo sayfasının tamamı. Dönen metin tek dosyadır: dış istek yok.
pub fn uret(g: &Girdi) -> String {
    let ic = icerik(g.tur);
    let ad = kacis(&g.ad);
    let sektor = kacis(&g.sektor);
    let adres = kacis(&g.adres);
    let bolge = ilce_metni(&g.adres);
    let harf =
        g.ad.chars()
            .next()
            .map(|harf| harf.to_uppercase().to_string())
            .unwrap_or_else(|| "•".to_string());
    let tel_hedef = g
        .telefon
        .as_ref()
        .map(|tel| format!("tel:+{}", tel.trim_start_matches('+')));

    let birincil = match &tel_hedef {
        Some(hedef) => format!(
            r##"<a class="buton buton-dolu" href="{hedef}">{TELEFON_IKON}{}</a>"##,
            kacis(ic.cta)
        ),
        None => String::new(),
    };
    let ikincil = if g.konum.is_some() {
        r##"<a class="buton buton-cizgi" href="#konum">Konumu gör</a>"##.to_string()
    } else {
        r##"<a class="buton buton-cizgi" href="#iletisim">İletişim</a>"##.to_string()
    };

    // Menü/hizmet listesi — her satır gecikmeli açılır.
    let mut liste = String::new();
    for (sira, (kalem, aciklama, fiyat)) in ic.kalemler.iter().enumerate() {
        liste.push_str(&format!(
            r##"<li class="satir" data-rise style="--g:{}">
<div class="satir-ust"><h3>{}</h3><span class="nokta"></span><span class="fiyat">{}</span></div>
<p>{}</p>
</li>"##,
            sira * 70,
            kacis(kalem),
            kacis(fiyat),
            kacis(aciklama)
        ));
    }

    let mut nedenler = String::new();
    for (sira, (baslik, metin)) in ic.neden.iter().enumerate() {
        nedenler.push_str(&format!(
            r##"<div class="kart" data-rise style="--g:{}">
<span class="sira">0{}</span>
<h3>{}</h3>
<p>{}</p>
</div>"##,
            sira * 90,
            sira + 1,
            kacis(baslik),
            kacis(metin)
        ));
    }

    let mut yorumlar = String::new();
    for (sira, (soz, kim)) in ic.yorumlar.iter().enumerate() {
        let bas_harf = kim.chars().next().unwrap_or('•').to_uppercase();
        yorumlar.push_str(&format!(
            r##"<blockquote class="alinti" data-rise style="--g:{}">
<div class="yildizlar">{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}</div>
<p>{}</p>
<footer><b>{bas_harf}</b><span>{}</span></footer>
</blockquote>"##,
            sira * 90,
            kacis(soz),
            kacis(kim)
        ));
    }

    let akis_parca = format!("<span>{}</span>", kacis(ic.akis));
    let akis = akis_parca.repeat(8);

    let puan_rozett = match (&g.puan, &g.yorum) {
        (Some(puan), Some(yorum)) => format!(
            r##"<div class="rozet rozet-b"><div class="yildizlar">{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}</div><div class="rozet-puan"><b>{puan}</b><small>{yorum}</small></div></div>"##
        ),
        (Some(puan), None) => format!(
            r##"<div class="rozet rozet-b"><div class="yildizlar">{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}{YILDIZ}</div><b>{puan}</b></div>"##
        ),
        // Puan yoksa ikinci rozet çizilmez: boş kutu göstergesi olmaz.
        _ => String::new(),
    };

    // Sahne: işletmenin fotoğrafı varsa o, yoksa tipografik kompozisyon.
    let sahne_ic = match &g.foto {
        Some((mime, veri)) => format!(
            r##"<img class="sahne-foto" src="data:{mime};base64,{veri}" alt="{ad}" loading="eager">
<div class="sahne-perde"></div>
<div class="sahne-koyu"></div>"##,
            ad = ad
        ),
        None => format!(
            r##"<div class="halkalar"></div>
<div class="monogram-buyuk">{harf}</div>"##,
            harf = kacis(&harf)
        ),
    };

    let harita = harita_html(g, &ad);
    let iletisim_kart = if harita.is_empty() {
        format!(
            r##"<div class="ilet-kart">{KONUM_IKON}<div><small>Adres</small><b>{adres}</b></div></div>"##
        )
    } else {
        harita
    };
    let tel_kart = match (&tel_hedef, &g.telefon_goster) {
        (Some(hedef), goster) => format!(
            r##"<a class="ilet-kart" href="{hedef}">{TELEFON_IKON}<div><small>Telefon</small><b>{}</b></div></a>"##,
            kacis(goster)
        ),
        _ => String::new(),
    };
    let konum_kart = if g.konum.is_some() {
        format!(
            r##"<div class="ilet-kart">{KONUM_IKON}<div><small>Adres</small><b>{adres}</b></div></div>"##
        )
    } else {
        String::new()
    };
    let karne = match &g.karne {
        Some(adres) => format!(
            r##"<a href="{}">Dijital görünürlük karnesi</a>"##,
            kacis(adres)
        ),
        None => String::new(),
    };
    let mobil_cta = match &tel_hedef {
        Some(hedef) => format!(
            r##"<a class="mobil-cta" href="{hedef}">{}</a>"##,
            kacis(ic.cta)
        ),
        None => String::new(),
    };
    let bant_buton = match &tel_hedef {
        Some(hedef) => format!(
            r##"<a class="buton" href="{hedef}">{TELEFON_IKON}{}</a>"##,
            kacis(ic.cta)
        ),
        None => String::new(),
    };

    // Harita altındaki rezervasyon kartı: sayfayı arayanı aramaya götürür.
    let harita_kart = match &tel_hedef {
        Some(hedef) => format!(
            r##"<div class="harita-kart"><div class="ince">İletişim</div><p style="margin:9px 0 16px;font-size:15px;line-height:1.6;color:var(--ink)">{cta} için arayın; {ad} adına not bırakalım.</p><a class="buton buton-dolu" href="{hedef}">{TELEFON_IKON}{cta}</a></div>"##,
            cta = kacis(ic.cta),
            ad = ad,
            hedef = hedef
        ),
        None => String::new(),
    };

    let mut sayfa = String::new();
    // Açık zeminde gölge/harita/ışık katmanlarının yönü değişir.
    let tema_acik = parlak_mi(&g.zemin);
    sayfa.push_str(if tema_acik {
        "<!DOCTYPE html>\n<html lang=\"tr\" class=\"tema-acik\">\n<head>\n<meta charset=\"utf-8\">\n"
    } else {
        "<!DOCTYPE html>\n<html lang=\"tr\">\n<head>\n<meta charset=\"utf-8\">\n"
    });
    sayfa.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    sayfa.push_str(&format!("<title>{ad} — {sektor} | {bolge}</title>\n"));
    let ozet = if adres.is_empty() {
        format!("{sektor} · {ad} · {}", ic.alt)
    } else {
        format!("{sektor} · {adres} · {}", ic.alt)
    };
    sayfa.push_str(&format!(
        concat!(
            "<meta name=\"description\" content=\"{ozet}\">\n",
            "<meta property=\"og:type\" content=\"website\">\n",
            "<meta property=\"og:site_name\" content=\"{ad}\">\n",
            "<meta property=\"og:title\" content=\"{ad} — {sektor}\">\n",
            "<meta property=\"og:description\" content=\"{ozet}\">\n",
            "<meta property=\"og:locale\" content=\"tr_TR\">\n",
            "<meta name=\"twitter:card\" content=\"summary\">\n",
            "<meta name=\"theme-color\" content=\"{vurgu}\">\n",
            "<meta name=\"robots\" content=\"noindex\">\n"
        ),
        ad = ad,
        sektor = sektor,
        ozet = kacis(&ozet),
        vurgu = g.vurgu
    ));
    sayfa.push_str("<style>\n");
    sayfa.push_str(g.font_css);
    sayfa.push_str(&format!(
        ":root{{--accent:{vurgu};--accent-karsi:{karsi};--bg:{zemin};--ink:{yazi};--f-baslik:'Space Grotesk','Inter',system-ui,-apple-system,'Segoe UI',sans-serif;--f-govde:'Inter',system-ui,-apple-system,'Segoe UI',sans-serif}}\n",
        vurgu = g.vurgu,
        zemin = g.zemin,
        yazi = g.yazi,
        karsi = if parlak_mi(&g.vurgu) { "#12100e" } else { "#f8f5f0" }
    ));
    sayfa.push_str(CSS);
    // JS kapalıysa içerik görünür kalır: açılım animasyonu beklemesin.
    sayfa.push_str(
        "</style>\n<noscript><style>[data-rise]{opacity:1;transform:none}.manset i{transform:none}</style></noscript>\n</head>\n<body>\n",
    );
    sayfa.push_str(&format!(
        r##"<div class="ilerleme" id="ilerleme" aria-hidden="true"></div>
<div class="serit">Taslak demo · {ad} için hazırlandı · fiyat ve yorumlar örnektir</div>
<header class="ust" id="ust">
<div class="ust-ic">
<a class="marka" href="#ust"><span class="monogram">{harf}</span><span>{ad}</span></a>
<nav class="menuler">
<a href="#hakkimizda">Hakkında</a>
<a href="#bolum">{bolum}</a>
{a_neden}
<a href="#iletisim">İletişim</a>
</nav>
{tel_dugme}
<button class="mobil-dugme" id="mobil-dugme" aria-expanded="false" aria-label="Menü">☰</button>
</div>
<nav class="mobil-menu" id="mobil-menu">
<a href="#hakkimizda">Hakkında</a>
<a href="#bolum">{bolum}</a>
<a href="#neden">Neden biz</a>
<a href="#iletisim">İletişim</a>
</nav>
</header>
"##,
        ad = ad,
        harf = kacis(&harf),
        bolum = kacis(ic.bolum),
        a_neden = r##"<a href="#neden">Neden biz</a>"##,
        tel_dugme = match &tel_hedef {
            Some(hedef) => format!(
                r##"<a class="ust-tel" href="{hedef}">{TELEFON_IKON}{}</a>"##,
                kacis(&g.telefon_goster)
            ),
            None => String::new(),
        }
    ));

    // Hero
    sayfa.push_str(&format!(
        r##"<section class="hero">
<div class="kabuk hero-ic">
<div class="heroi">
<span class="rozet-cip"><b></b>{sektor} · {bolge}</span>
<h1 class="manset">
<span><i style="--i:0">{satir1}</i></span>
<span><i style="--i:1">{satir2}</i></span>
</h1>
<p class="hero-alt">{alt}</p>
<div class="hero-butonlar">{birincil}{ikincil}</div>
<div class="durum"><span class="nokta-canli" id="durum-nokta"></span><span class="durum-metin" id="durum-metin">Çalışma saatleri yükleniyor…</span></div>
</div>
<div class="{sahne_sinif}" data-par="0.05">
{sahne_ic}
<div class="sahne-alt"><span>{sektor}</span><span>{bolge}</span></div>
<div class="rozet rozet-a"><small>Bugün</small><b id="rozet-saat">—</b></div>
{puan_rozett}
</div>
</div>
{istatistik}
</section>
<div class="akis"><div class="akis-ic">{akis}{akis}</div></div>
"##,
        sektor = sektor,
        satir1 = kacis(ic.manset[0]),
        satir2 = kacis(ic.manset[1]),
        alt = kacis(ic.alt),
        birincil = birincil,
        ikincil = ikincil,
        sahne_sinif = if g.foto.is_some() { "sahne foto" } else { "sahne" },
        sahne_ic = sahne_ic,
        bolge = bolge,
        puan_rozett = puan_rozett,
        istatistik = istatistik_html(g, &ic),
        akis = akis
    ));

    // Hakkında + listeler + neden biz + yorumlar
    sayfa.push_str(&format!(
        r##"<section class="bolum" id="hakkimizda"><div class="kabuk">
<div class="bolum-ust" data-rise><span class="no">01</span><span class="ince">Hakkında</span></div>
<div class="hikaye">
<div><h2 data-rise style="--g:60">{hikaye_baslik}</h2></div>
<div>
<p class="hikaye-buyuk" data-rise style="--g:120">{hikaye1}</p>
<p data-rise style="--g:180">{hikaye2}</p>
</div>
</div>
</div></section>

<section class="bolum" id="bolum"><div class="kabuk">
<div class="bolum-ust" data-rise><span class="no">02</span><span class="ince">{bolum_etiket}</span></div>
<h2 data-rise style="--g:60">{bolum}</h2>
<p class="hero-alt" data-rise style="--g:110" style="margin-top:12px">{bolum_alt}</p>
<ul class="liste" data-rise style="--g:150;margin-top:34px">{liste}</ul>
<p class="liste-alt">Fiyatlar örnektir; işletmenin gerçek listesiyle değiştirilecek.</p>
</div></section>

<section class="bolum" id="neden"><div class="kabuk">
<div class="bolum-ust" data-rise><span class="no">03</span><span class="ince">Neden biz</span></div>
<div class="uc">{nedenler_kart}</div>
</div></section>

<section class="bolum"><div class="kabuk">
<div class="bolum-ust" data-rise><span class="no">04</span><span class="ince">Yorumlar</span></div>
<div class="yorumlar">{yorumlar}</div>
<p class="ornek-not">Yorumlar örnektir · gerçek yorumlarla değiştirilecek</p>
</div></section>
"##,
        hikaye_baslik = kacis(ic.hikaye_baslik),
        hikaye1 = kacis(ic.hikaye[0]),
        hikaye2 = kacis(ic.hikaye[1]),
        bolum_etiket = kacis(ic.etiket),
        bolum = kacis(ic.bolum),
        bolum_alt = kacis(ic.bolum_alt),
        liste = liste,
        nedenler_kart = nedenler,
        yorumlar = yorumlar
    ));

    sayfa.push_str(&format!(
        r##"<section class="bolum" id="iletisim"><div class="kabuk">
<div class="bolum-ust" data-rise><span class="no">05</span><span class="ince">Saatler &amp; iletişim</span></div>
<div class="iletisim">
<div>
<h2 data-rise style="--g:50">Ne zaman gelirseniz?</h2>
<div data-rise style="--g:110;margin-top:24px">{saatler}</div>
<div class="ilet-kartlar">{tel_kart}{konum_kart}</div>
</div>
<div id="konum" data-rise style="--g:80">{harita_bolum}{harita_kart}</div>
</div>
</div></section>

<section class="bant"><div class="kabuk bant-ic">
<div><h2>{cta} — bir telefon kadar yakın.</h2><p>{ad} · {bolge}</p></div>
{bant_buton}
</div></section>

<footer class="altbilgi"><div class="kabuk alt-ic">
<div>
<div class="alt-marka"><span class="monogram">{harf}</span>{ad}</div>
<p class="alt-not">Bu taslak {gonderen} tarafından {ad} için hazırlandı. Fiyatlar ve yorumlar örnektir; adres ve saat bilgileri {kaynak} kaydından alınmıştır.{foto_not}</p>
</div>
<div class="alt-baglar"><a href="#hakkimizda">Hakkında</a><a href="#bolum">{bolum}</a><a href="#iletisim">İletişim</a>{karne}</div>
</div></footer>
{mobil_cta}
<div id="fener" aria-hidden="true"></div>
<div class="gren" aria-hidden="true"></div>
"##,
        cta = kacis(ic.cta),
        ad = ad,
        bolge = bolge,
        bant_buton = bant_buton,
        gonderen = kacis(&g.gonderen),
        kaynak = kacis(&g.kaynak),
        foto_not = match &g.foto_atif {
            Some(atif) if !atif.trim().is_empty() => format!(" Fotoğraf: {}.", kacis(atif)),
            _ => String::new(),
        },
        harf = kacis(&harf),
        bolum = kacis(ic.bolum),
        karne = karne,
        mobil_cta = mobil_cta,
        saatler = saatler_html(g),
        tel_kart = tel_kart,
        konum_kart = konum_kart,
        harita_bolum = iletisim_kart,
        harita_kart = harita_kart
    ));

    // Demo süresi: ilk açılıştan 12 saat sonra sayfa kapanır.
    sayfa.push_str(
        "<div class=\"suresi-doldu\" id=\"suresi-doldu\" hidden>\n<p>Bu demonun süresi doldu.<br>Güncel bir demo için işletmeyle iletişime geçin.</p>\n</div>\n",
    );
    let program = g.program.clone().unwrap_or_else(|| "null".to_string());
    sayfa.push_str(&SCRIPTS.replace("__PROGRAM__", &program));
    sayfa.push_str(&format!(
        concat!(
            "<script>\n",
            "(function(){{\n",
            "  var a='demo_{anahtar}', b=Number(localStorage.getItem(a)||Date.now());\n",
            "  localStorage.setItem(a,b);\n",
            "  var sure=document.getElementById('suresi-doldu');\n",
            "  if(sure && Date.now()-b>12*60*60*1000){{sure.hidden=false;document.body.style.overflow='hidden';}}\n",
            "}})();\n",
            "</script>\n</body>\n</html>\n"
        ),
        anahtar = g.anahtar
    ));
    sayfa
}

/// Sayfanın davranışı: açılım, parallax, canlı açık/kapalı durumu, mobil menü.
const SCRIPTS: &str = r##"<script>
(function () {
  var PROGRAM = __PROGRAM__;
  var GUNLER = ["Pazartesi","Salı","Çarşamba","Perşembe","Cuma","Cumartesi","Pazar"];
  var azalt = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function iki(sayi) { return String(sayi).padStart(2, "0"); }
  function saatMetni(dk) { return iki(Math.floor(dk / 60) % 24) + ":" + iki(dk % 60); }
  function kalanMetni(dk) {
    var gun = Math.floor(dk / 1440), saat = Math.floor((dk % 1440) / 60), d = dk % 60;
    var p = [];
    if (gun > 0) p.push(gun + " gün");
    if (saat > 0) p.push(saat + " sa");
    if (d > 0 && gun === 0) p.push(d + " dk");
    return p.length ? p.join(" ") : "0 dk";
  }

  // 1) Üst menü daralması + okuma ilerlemesi
  var ust = document.getElementById("ust");
  var ilerleme = document.getElementById("ilerleme");
  function kaydirma() {
    if (ust) ust.classList.toggle("kucuk", window.scrollY > 12);
    if (ilerleme) {
      var toplam = document.documentElement.scrollHeight - window.innerHeight;
      var oran = toplam > 0 ? Math.min(1, window.scrollY / toplam) : 0;
      ilerleme.style.width = (oran * 100).toFixed(2) + "%";
    }
  }
  kaydirma();
  window.addEventListener("scroll", kaydirma, { passive: true });

  // 2) Mobil menü
  var dugme = document.getElementById("mobil-dugme"), menu = document.getElementById("mobil-menu");
  if (dugme && menu) {
    dugme.addEventListener("click", function () {
      var acik = menu.classList.toggle("acik");
      dugme.setAttribute("aria-expanded", acik ? "true" : "false");
    });
    menu.querySelectorAll("a").forEach(function (a) {
      a.addEventListener("click", function () { menu.classList.remove("acik"); });
    });
  }

  // 3) Kaydırmayla açılan bölümler
  var ogeler = [].slice.call(document.querySelectorAll("[data-rise]"));
  if (!azalt && "IntersectionObserver" in window) {
    var gozcu = new IntersectionObserver(function (girisler) {
      girisler.forEach(function (giris) {
        if (!giris.isIntersecting) return;
        giris.target.classList.add("in");
        gozcu.unobserve(giris.target);
      });
    }, { threshold: 0.12, rootMargin: "0px 0px -6% 0px" });
    ogeler.forEach(function (oge) { gozcu.observe(oge); });
  } else {
    ogeler.forEach(function (oge) { oge.classList.add("in"); });
  }

  // 4) Parallax
  var parlar = [].slice.call(document.querySelectorAll("[data-par]"));
  if (!azalt && parlar.length && window.matchMedia("(hover: hover)").matches) {
    var bekliyor = false;
    function parala() {
      bekliyor = false;
      parlar.forEach(function (oge) {
        var kutu = oge.getBoundingClientRect();
        var oran = (kutu.top + kutu.height / 2 - window.innerHeight / 2) * -Number(oge.dataset.par || 0);
        oge.style.transform = "translate3d(0," + oran.toFixed(1) + "px,0)";
      });
    }
    window.addEventListener("scroll", function () {
      if (bekliyor) return;
      bekliyor = true;
      requestAnimationFrame(parala);
    }, { passive: true });
    parala();
  }

  // 5) Canlı açık/kapalı durumu + bugünün saati
  var metin = document.getElementById("durum-metin");
  var nokta = document.getElementById("durum-nokta");
  var rozet = document.getElementById("rozet-saat");
  var istBugun = document.getElementById("ist-bugun");
  var gunler = document.querySelectorAll(".saat-satir[data-gun]");

  function bugunAralik(gun) {
    var araliklar = (PROGRAM && PROGRAM[gun]) || [];
    if (!araliklar.length) return null;
    return araliklar[0];
  }

  function durumYaz(metinDeger, durum) {
    if (metin) { metin.textContent = metinDeger; metin.className = "durum-metin " + durum; }
    if (nokta) {
      nokta.className = "nokta-canli" + (durum === "kapali" ? " kapali" : (durum === "yakin" ? " yakin" : ""));
    }
  }

  function durumHesapla() {
    if (!PROGRAM) {
      if (metin && !metin.textContent.trim()) metin.textContent = "Çalışma saatleri için arayın";
      return;
    }
    var simdi = new Date();
    var gun = (simdi.getDay() + 6) % 7;
    var dk = simdi.getHours() * 60 + simdi.getMinutes();
    var aralik = bugunAralik(gun);

    if (rozet) rozet.textContent = aralik ? (saatMetni(aralik[0]) + " – " + saatMetni(aralik[1])) : "Kapalı";
    if (istBugun) istBugun.textContent = aralik ? (saatMetni(aralik[0]) + "–" + saatMetni(aralik[1])) : "Kapalı";
    gunler.forEach(function (satir) {
      satir.classList.toggle("bugun", Number(satir.dataset.gun) === gun);
    });

    for (var i = 0; i < (PROGRAM[gun] || []).length; i++) {
      var bas = PROGRAM[gun][i][0], son = PROGRAM[gun][i][1];
      if (dk >= bas && dk < son) {
        var kalan = son - dk;
        if (kalan <= 15) {
          durumYaz("Kapanmak üzere · kapanış " + saatMetni(son) + " · " + kalanMetni(kalan) + " kaldı", "yakin");
        } else {
          durumYaz("Şu an açık · kapanış " + saatMetni(son) + " · " + kalanMetni(kalan) + " kaldı", "acik");
        }
        return;
      }
    }
    for (var g = 0; g < 7; g++) {
      var hedef = (gun + g) % 7;
      var liste = PROGRAM[hedef] || [];
      for (var k = 0; k < liste.length; k++) {
        var acilis = liste[k][0];
        if (g === 0 && acilis <= dk) continue;
        var etiket = g === 0 ? "bugün" : (g === 1 ? "yarın" : GUNLER[hedef].toLocaleLowerCase("tr"));
        var kalanDk = g * 1440 + acilis - dk;
        durumYaz("Şu an kapalı · açılış " + etiket + " " + saatMetni(acilis) + " · " + kalanMetni(kalanDk) + " kaldı", "kapali");
        return;
      }
    }
    durumYaz("Şu an kapalı", "kapali");
  }
  durumHesapla();
  if (PROGRAM) setInterval(durumHesapla, 60000);

  // 6) İmleci izleyen ışık (yalnızca fare olan cihazlarda)
  var fener = document.getElementById("fener");
  if (fener && !azalt && window.matchMedia("(hover: hover) and (pointer: fine)").matches) {
    var sirada = false, fx = 0, fy = 0;
    window.addEventListener("pointermove", function (olay) {
      if (olay.pointerType === "touch") return;
      fx = olay.clientX; fy = olay.clientY;
      fener.classList.add("acik");
      if (sirada) return;
      sirada = true;
      requestAnimationFrame(function () {
        sirada = false;
        fener.style.setProperty("--fx", fx + "px");
        fener.style.setProperty("--fy", fy + "px");
      });
    }, { passive: true });
    document.addEventListener("pointerleave", function () { fener.classList.remove("acik"); });
  }
})();
</script>
"##;
