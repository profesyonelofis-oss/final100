# -*- coding: utf-8 -*-
"""OTOMATIK YEDEKLEYICI - Render silinmelerine karsi gunluk yedek.

Ne yapar?
  Render uzerindeki KARAOGLU Zeytin Takip sitesinden veritabanini (zeytin_takip.db)
  indirip bu bilgisayarda C:\\KARAOGLU_YEDEK klasorune kaydeder.
  Son 30 gunun yedegini tutar, eskileri otomatik siler.

Nasil calisir?
  YEDEK_AL.bat dosyasina cift tiklayin -> yedek alir, sonucu gosterir.
  Windows Gorev Zamanlayici'ya eklendiginde HER GUN OTOMATIK calisir
  (ayari YEDEK_KUR.bat yapar; silmek icin YEDEK_SIL.bat).

Ayarlar (asagidaki sabitleri degistirin):
  SITE_ADRESI : Render'daki site adresiniz (sonunda / olmasin)
  YEDEK_KLASORU : Yedeklerin kaydedilecegi klasor
  SAKLAMA_GUN : Kac gunluk yedek tutulacak
"""

import os
import sys
import time
import urllib.request

# ------------------------------------------------- AYARLAR (burayi duzenleyin)
SITE_ADRESI = "https://www.zeytinhesap.com"  # Site adresiniz (Render/PythonAnywhere)
YEDEK_KLASORU = r"C:\KARAOGLU_YEDEK"
SAKLAMA_GUN = 30
# -----------------------------------------------------------------------------

# Gizli anahtar: local data/secret_key dosyasindan okunur (Render'a yuklerken
# data/ gitmeshese bile bilgisayarinizda varsa otomatik bulunur).
BULUNDUGU_KLASOR = os.path.dirname(os.path.abspath(__file__))


def _anahtar_bul():
    """Yedek anahtarini bulur: 1) ortam degiskeni 2) data/secret_key 3) kullanicidan."""
    env = os.environ.get("KARAOGLU_SECRET_KEY")
    if env:
        return env.strip()
    for aday in (os.path.join(BULUNDUGU_KLASOR, "data", "secret_key"),
                 os.path.join(BULUNDUGU_KLASOR, "secret_key")):
        try:
            with open(aday, "r", encoding="utf-8") as f:
                icerik = f.read().strip()
                if icerik:
                    return icerik
        except OSError:
            pass
    return None


def _yedek_zamani_yaz(zaman):
    try:
        with open(os.path.join(YEDEK_KLASORU, "son_yedek.txt"), "w",
                  encoding="utf-8") as f:
            f.write(zaman)
    except OSError:
        pass


def gunluk_yedek_al():
    anahtar = _anahtar_bul()
    if not anahtar:
        print("HATA: Gizli anahtar bulunamadi!")
        print("  KARAOGLU_SECRET_KEY ortam degiskenini ayarlayin ya da")
        print("  data/secret_key dosyasini bu klasore kopyalayin.")
        return 2

    if not os.path.isdir(YEDEK_KLASORU):
        os.makedirs(YEDEK_KLASORU, exist_ok=True)

    adres = SITE_ADRESI.rstrip("/") + "/yedek-otomatik?anahtar=" + anahtar
    stamp = time.strftime("%Y%m%d_%H%M%S")
    hedef = os.path.join(YEDEK_KLASORU, "zeytin_takip_yedek_%s.db" % stamp)

    print("Yedek aliniyor:", SITE_ADRESI)
    try:
        istek = urllib.request.Request(adres, headers={"User-Agent": "karaoglu-yedek/1"})
        with urllib.request.urlopen(istek, timeout=120) as cevap:
            if cevap.status != 200:
                print("HATA: Sunucu %s dondu." % cevap.status)
                return 1
            veri = cevap.read()
    except Exception as e:
        print("HATA: Siteye baglanilamadi:", e)
        return 1

    # Bozuk/cok kucuk dosya kontrolu (gercek DB en az birkac KB olur)
    if len(veri) < 4096:
        print("HATA: Inen dosya cok kucuk (%d bayt) - alinmadi." % len(veri))
        return 1

    with open(hedef, "wb") as f:
        f.write(veri)
    print("Yedek alindi:", hedef, "(%s KB)" % (len(veri) // 1024))

    _yedek_zamani_yaz(time.strftime("%Y-%m-%d %H:%M:%S"))

    # Eskileri sil
    simdi = time.time()
    silinen = 0
    for ad in os.listdir(YEDEK_KLASORU):
        if ad.startswith("zeytin_takip_yedek_") and ad.endswith(".db"):
            yol = os.path.join(YEDEK_KLASORU, ad)
            try:
                if simdi - os.path.getmtime(yol) > SAKLAMA_GUN * 86400:
                    os.remove(yol)
                    silinen += 1
            except OSError:
                pass
    if silinen:
        print("%d eski yedek silindi (saklama suresi: %d gun)." % (silinen, SAKLAMA_GUN))

    kalan = [a for a in os.listdir(YEDEK_KLASORU)
             if a.startswith("zeytin_takip_yedek_") and a.endswith(".db")]
    print("Toplam %d yedek dosyasi var." % len(kalan))
    return 0


if __name__ == "__main__":
    sys.exit(gunluk_yedek_al())
