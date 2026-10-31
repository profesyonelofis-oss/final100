# -*- coding: utf-8 -*-
"""Firma adina gore otomatik SVG logo uretimi.

Dosya yuklemeye gerek yok: firma adindan 5 farkli stil uretilir,
kullanici Ayarlar > Logo ekranindan birini secer. Secilen logo
kucuk bir SVG dosyasi olarak data/ klasorune yazilir.

Harf bicimleri: monogram (bas harfler) + kisa is.
"""
from __future__ import annotations

import hashlib
import re

# 5 stil: (renk cifti, arka plan sekli, yazi rengi, bicim)
# renkler: zeytin yesili tonlari etrafinda
_STILLER = [
    {"ad": "Klasik",    "bg1": "#1F6B3B", "bg2": "#14522C", "yazi": "#FFFFFF", "sekil": "yuvarlak"},
    {"ad": "Koyu",      "bg1": "#0F3D23", "bg2": "#0A2B18", "yazi": "#E8F5EC", "sekil": "yuvarlak"},
    {"ad": "Yesil Kare", "bg1": "#2E8B57", "bg2": "#1F6B3B", "yazi": "#FFFFFF", "sekil": "kare"},
    {"ad": "Beyaz",     "bg1": "#FFFFFF", "bg2": "#F0F4F1", "yazi": "#1F6B3B", "sekil": "kare", "kenar": "#1F6B3B"},
    {"ad": "Altin",     "bg1": "#B8860B", "bg2": "#8B6508", "yazi": "#FFF8E7", "sekil": "yuvarlak"},
]

kenar = None  # kullanilmayan yerel degisken isaretci


def _monogram(firma_adi: str) -> str:
    """Firma adindan 1-2 harflik monogram uretir."""
    kelimeler = re.findall(r"[0-9A-Za-zÇĞİÖŞÜçğıöşü]+", (firma_adi or "").strip())
    if not kelimeler:
        return "K"
    if len(kelimeler) == 1:
        k = kelimeler[0]
        return (k[:2] if len(k) >= 2 else k).upper()
    return (kelimeler[0][0] + kelimeler[1][0]).upper()


def _kisa_ad(firma_adi: str, limit: int = 12) -> str:
    ad = (firma_adi or "").strip() or "KARAOĞLU"
    if len(ad) <= limit:
        return ad
    return ad[:limit - 1].rstrip() + "…"


def _svg_uret(firma_adi: str, stil: dict, boyut: int = 256) -> str:
    """Bir stilin SVG kaynagini uretir."""
    mono = _monogram(firma_adi)
    kisa = _kisa_ad(firma_adi)
    yazi_boyut = 96 if len(mono) == 1 else 82
    alt_boyut = 26

    if stil["sekil"] == "yuvarlak":
        arka = (f'<circle cx="128" cy="118" r="112" fill="url(#g)"/>')
    else:
        arka = (f'<rect x="16" y="6" width="224" height="224" rx="40" fill="url(#g)"/>')

    kenar_svg = ""
    if stil.get("kenar"):
        kenar_svg = f' stroke="{stil["kenar"]}" stroke-width="6"'
        # sekil elemanina eklemek icin yeniden uret
        if stil["sekil"] == "yuvarlak":
            arka = f'<circle cx="128" cy="118" r="109" fill="url(#g)"{kenar_svg}/>'
        else:
            arka = f'<rect x="19" y="9" width="218" height="218" rx="38" fill="url(#g)"{kenar_svg}/>'

    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="{boyut}" height="{boyut}" role="img" aria-label="{kisa} logo">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{stil['bg1']}"/>
      <stop offset="1" stop-color="{stil['bg2']}"/>
    </linearGradient>
  </defs>
  {arka}
  <text x="128" y="128" text-anchor="middle" dominant-baseline="central"
        font-family="Arial, Helvetica, sans-serif" font-weight="700"
        font-size="{yazi_boyut}" fill="{stil['yazi']}">{mono}</text>
  <text x="128" y="196" text-anchor="middle"
        font-family="Arial, Helvetica, sans-serif" font-weight="600"
        font-size="{alt_boyut}" fill="{stil['yazi']}" opacity="0.92">{kisa}</text>
</svg>'''


def tum_stiller(firma_adi: str) -> list[dict]:
    """5 logonun (ad + SVG) listesini dondurur."""
    return [{"stil": s["ad"], "svg": _svg_uret(firma_adi, s)} for s in _STILLER]


def secili_stil_adi() -> str:
    """Sablon default'u icin stil adlari (sabit liste)."""
    return [s["ad"] for s in _STILLER]


def onerilen_stil(firma_adi: str) -> str:
    """Firma adina deterministik oneri (ayni ad -> ayni stil)."""
    idx = int(hashlib.sha256((firma_adi or "").encode("utf-8")).hexdigest(), 16) % len(_STILLER)
    return _STILLER[idx]["ad"]


def svg_ure(firma_adi: str, stil_adi: str) -> str:
    """Secilen stilin SVG'sini dondurur; bilinmeyen stil ilk stili verir."""
    for s in _STILLER:
        if s["ad"] == stil_adi:
            return _svg_uret(firma_adi, s)
    return _svg_uret(firma_adi, _STILLER[0])
