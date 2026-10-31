# -*- coding: utf-8 -*-
"""Veritabanini sifirlar: tum islem verilerini siler, yalnizca admin kalir.

KULLANIM (Render One-Off Job veya Shell):
    python admin_sifirla.py            -> sadece GOSTERIR, silmez (guvenli onizleme)
    python admin_sifirla.py --evet     -> gercekten siler (once otomatik yedek alir)

Silinenler: alim/satis fisleri, detaylari, odemeler, giderler, satici verileri,
teslimat notlari, kalibre fiyatlari, admin olmayan kullanicilar, cihaz kayitlari,
odeme bildirimleri, bildirim okundu durumlari.
Korunanlar: admin kullanicilar, genel_ayarlar (firma adi, ucret, IBAN vb.).
"""
import os
import sys
import sqlite3
import shutil
from datetime import datetime

DB_YOL = os.environ.get("SIFIRLA_DB", "zeytin_takip.db")

TEMIZLENECEK_TABLOLAR = [
    "alim_detay", "alim_fisi", "alimlar",
    "satis_detay", "satis_fisi",
    "odemeler", "giderler",
    "kalibre_fiyatlari", "teslimat_notlari",
    "cihaz_kayitlari", "cihaz_izinleri",
    "odeme_bildirimleri",
]


def tablo_var_mi(conn, ad):
    r = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
        (ad,)).fetchone()
    return r is not None


def main():
    evet = "--evet" in sys.argv
    if not os.path.exists(DB_YOL):
        print("HATA: Veritabani bulunamadi:", DB_YOL)
        sys.exit(1)

    conn = sqlite3.connect(DB_YOL)

    print("=" * 60)
    print("SIFIRLAMA ONIZLEME -", DB_YOL)
    print("=" * 60)

    # Kullanicilar
    print("\nKULLANICILAR:")
    for (ad, rol, bitis) in conn.execute(
            "SELECT kullanici_adi, rol, lisans_bitis FROM kullanicilar").fetchall():
        isaret = "[KORUNUR]" if rol == "admin" else "[SILINECEK]"
        print("  %s %-20s rol=%-10s bitis=%s" % (isaret, ad, rol, bitis))

    # Islem verileri
    print("\nISLEM VERILERI:")
    toplam = 0
    for t in TEMIZLENECEK_TABLOLAR:
        if tablo_var_mi(conn, t):
            n = conn.execute("SELECT COUNT(*) FROM %s" % t).fetchone()[0]
            toplam += n
            print("  %-22s %6d kayit  [SILINECEK]" % (t, n))
    print("  Toplam silinecek islem kaydi: %d" % toplam)

    # Korunan ayarlar
    print("\nKORUNAN AYARLAR (genel_ayarlar):")
    if tablo_var_mi(conn, "genel_ayarlar"):
        for (k, v) in conn.execute(
                "SELECT anahtar, deger FROM genel_ayarlar LIMIT 10").fetchall():
            print("  %-22s %s" % (k, (v or "")[:40]))

    if not evet:
        print("\n>>> SADECE ONIZLEME. Silmek icin: python admin_sifirla.py --evet")
        conn.close()
        return

    # Yedek
    yedek_ad = "yedek_sifirlama_once_%s.db" % datetime.now().strftime("%Y%m%d_%H%M%S")
    shutil.copy2(DB_YOL, yedek_ad)
    print("\nYEDEK ALINDI -> %s" % yedek_ad)

    # Sil
    for t in TEMIZLENECEK_TABLOLAR:
        if tablo_var_mi(conn, t):
            conn.execute("DELETE FROM %s" % t)
            print("  silindi: %s" % t)
    conn.execute("DELETE FROM kullanicilar WHERE rol != 'admin'")
    print("  silindi: kullanicilar (admin haric)")
    if tablo_var_mi(conn, "sqlite_sequence"):
        for t in TEMIZLENECEK_TABLOLAR:
            conn.execute("DELETE FROM sqlite_sequence WHERE name=?", (t,))
        conn.execute("DELETE FROM sqlite_sequence WHERE name='kullanicilar'")
        print("  sifirlandi: otomatik ID sayaclari")
    conn.commit()
    conn.close()
    print("\n*** TAMAM: Veritabani sifirlandi. Yalnizca admin hesabi kaldi. ***")


if __name__ == "__main__":
    main()
