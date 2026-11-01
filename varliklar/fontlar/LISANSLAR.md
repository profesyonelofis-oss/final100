# Gömülü fontlar

Demo sayfaları bu iki fontu **base64 olarak kendi içinde** taşır; dış istek yok.

| Dosya | Font | Lisans | Kaynak |
|---|---|---|---|
| `space-grotesk-tr.woff2` | Space Grotesk (değişken, wght 300–700) | SIL Open Font License 1.1 | https://github.com/google/fonts/tree/main/ofl/spacegrotesk |
| `inter-tr.woff2` | Inter (değişken, wght 100–900) | SIL Open Font License 1.1 | https://github.com/google/fonts/tree/main/ofl/inter |

## Neden bu ikili

- **Space Grotesk** başlıklarda: geometrik, karakterli, "ajans işi" hissi veriyor.
- **Inter** gövdede: küçük puntoda çok okunaklı, ekran için tasarlanmış.
- İkisi de Türkçe karakterlerin tamamını (ı İ ş Ş ğ Ğ ç Ç ö Ö ü Ü) içeriyor.
- İkisi de **SIL OFL 1.1**: ticari kullanım serbest, müşteri işine gömülebilir,
  atıf zorunlu değil (lisans metni dağıtılan pakette kalmalı).

## Nasıl üretildi (tekrar gerekirse)

```bash
# 1) Google Fonts deposundan değişken (variable) TTF
curl -L -o space-grotesk-var.ttf \
  "https://github.com/google/fonts/raw/main/ofl/spacegrotesk/SpaceGrotesk%5Bwght%5D.ttf"
curl -L -o inter-var.ttf \
  "https://github.com/google/fonts/raw/main/ofl/inter/Inter%5Bopsz%2Cwght%5D.ttf"

# 2) Türkçe + ASCII alt kümesi (woff2)
.venv/Scripts/python.exe -m fontTools.subset space-grotesk-var.ttf \
  --text-file=harfler.txt --flavor=woff2 --output-file=space-grotesk-tr.woff2 \
  --layout-features=kern,liga,calt --no-hinting
```

Alt kümede tutulan karakterler: ASCII (0x20–0x7E), Türkçe harfler, tırnak/çizgi
işaretleri ve ₺ € → ← ✓ gibi simgeler — toplam 142 karakter.

Boyut: Space Grotesk **14 KB**, Inter **43 KB** (base64'e çevrilince ~76 KB).
