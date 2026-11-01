# Şablon kütüphanesi (demo üretimi için)

Bu klasör, demo üretirken **tasarım referansı** olarak kullanılan, lisansı temiz
şablonları tutar. Buradaki dosyalar doğrudan kopyalanıp müşteriye satılmaz;
bölüm sırası, boşluk ölçeği, tipografi ve bileşen fikri alınır — HTML kendi
üreticimizde yeniden yazılır.

## Neden bu kaynaklar

Videodaki ("cinematic website") tarzın çalışan, ücretsiz karşılığı şu üç kaynaktan
geliyor. Hepsi **MIT** lisanslı: ticari kullanım serbest, atıf zorunlu değil.

| Klasör | Kaynak | Lisans | Ne için |
|---|---|---|---|
| `startbootstrap-creative/` | https://github.com/StartBootstrap/startbootstrap-creative | MIT | Diş kliniği, kuaför, düğün — büyük başlık + galeri + iletişim |
| `startbootstrap-one-page-wonder/` | https://github.com/StartBootstrap/startbootstrap-one-page-wonder | MIT | Oto servis, çekici, emlak — sıralı bölüm akışı |
| `tailwind-toolbox-landing/` | https://github.com/tailwindtoolbox/Landing-Page | MIT (kod) | Restoran, kafe, herhangi bir landing — Tailwind grid |

## Kullanım kuralları (önemli)

1. **Kopyala-yapıştır satış yok.** Şablonlar tasarım referansı; çıktımız tek
   dosyalık, kendi ürettiğimiz HTML.
2. **Demo görselleri kopyalanmaz.** Start Bootstrap şablonlarındaki `assets/`
   klasörünü bilerek almadık; Tailwind Toolbox'ın `hero.png` dosyası Freepik
   kaynaklı ve atıf istiyor — o yüzden o da alınmadı. Fotoğraflarımız kendi
   kaynağımızdan gelir.
3. **Lisans dosyaları silinmez.** Her klasördeki `LICENSE` olduğu gibi durur.
4. **Yasak kaynaklar** (araştırmayla doğrulandı):
   - **BootstrapMade ücretsiz** — "Designed by BootstrapMade" footer kredisi
     zorunlu ve *"müşteri işi için kullanamazsınız"* maddesi var.
   - **"1000 free HTML templates.zip" türü toplu paketler** — lisans zinciri
     kopuk, içinde ücretli şablonlar ve eski/kırık dosyalar var. Kullanmayın.
   - **ThemeWagon** — atıf kuralı tema bazında değişiyor; kullanılacaksa her
     temanın kendi notunu okuyun.
5. **HTML5 UP** tasarımları çok iyi ama **CC BY 3.0**: footer'da atıf zorunlu.
   Müşteri işinde atıf istemiyorsak kullanmayız.

## Sektöre göre hangi referanstan ilham alıyoruz

| Sektör | Referans | Alınan fikir |
|---|---|---|
| Restoran, kafe, meyhane | `tailwind-toolbox-landing` | Menü ızgarası, iki sütun hero, "örnek menü" notu |
| Kuaför, güzellik, düğün | `startbootstrap-creative` | Kare galeri + ortalanmış büyük başlık + tek tıkla arama |
| Klinik, diş, estetik | `startbootstrap-creative` | Hizmet kartları, güven şeridi, randevu CTA |
| Çekici, oto servis, tamir | `startbootstrap-one-page-wonder` | Sıralı akış (acil → hizmet → kapsam → iletişim), numara öne |
| Emlak, danışmanlık | `startbootstrap-one-page-wonder` | Liste + harita + iletişim bloğu |
| Otel, pansiyon | `startbootstrap-creative` | Geniş görsel, olanaklar ızgarası |

## "Pahalı görünen site" kontrol listesi

Videodaki sinematik sitenin ayırt edici öğeleri (ve bizde karşılıkları):

| Öğe | Nasıl yapılır | Bizde durumu |
|---|---|---|
| El feneri (flashlight) etkileşimi | İmleci takip eden radyal ışık katmanı | ✓ demo şablonunda var (yalnız fare olan cihazlarda) |
| Büyük, sıkı tipografi | `clamp()` ile ölçeklenen başlık, dar satır aralığı | ✓ |
| Nefes alan boşluk | Bölüm arası 80–112 px, kart içi 24–32 px | ✓ |
| Koyu, atmosferik zemin | Neredeyse siyah (`#08090d`) + sektör rengi vurgu | ✓ |
| Hareket | Kaydırmayla paralaks/ölçek, görünür olunca açılma | ✓ |
| Fotoğraf | Gerçek görsel, `object-fit: cover`, WebP | ✓ (işletme fotoğrafı girilince) |
| Tek eylem | Her ekranda tek belirgin CTA (ara / WhatsApp) | ✓ |
| Yükleme hızı | Tek dosya, dış bağımlılık yok | ✓ (Tailwind CDN hariç) |

Eksik olan tek şey **özel (self-hosted) font**: şu an sistem font yığını
kullanılıyor. Satışta en çok fark yaratan kalem bu.
