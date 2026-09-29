# -*- coding: utf-8 -*-
"""KARAOGLU Zeytin Takip - hizli duman testi.

Yerel sunucuyu GECICI veritabaniyla ayaga kaldirir, tum ana akisi
saniyeler icinde test eder ve kapanir. Yerel data/ klasorune dokunmaz.

Calistirma:
    .venv\\Scripts\\python.exe hizli_test.py
"""
import os
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

PORT = 5099
BASE = f"http://127.0.0.1:{PORT}"
KOK = Path(__file__).resolve().parent

PASS, FAIL = [], []


def kontrol(ad, kosul, detay=""):
    (PASS if kosul else FAIL).append(ad + (f"  [{detay}]" if detay and not kosul else ""))
    print(("  OK   " if kosul else "  HATA ") + ad + (f"  ({detay})" if detay and not kosul else ""))


def istek(yol, veri=None):
    """GET/POST atar; (durum, govde, son_url) dondurur."""
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *a, **k):
            return None
    ac = urllib.request.build_opener(NoRedirect)
    veri_bytes = urllib.parse.urlencode(veri).encode() if veri else None
    req = urllib.request.Request(BASE + yol, data=veri_bytes, method="POST" if veri else "GET")
    try:
        r = ac.open(req, timeout=10)
        return r.status, r.read().decode("utf-8", "replace"), r.url
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace"), e.headers.get("Location", "")


import urllib.error  # noqa: E402
import urllib.parse  # noqa: E402


def main():
    gecici = tempfile.mkdtemp(prefix="zeytin_test_")
    ort = dict(os.environ)
    ort["KARAOGLU_DATA_DIR"] = gecici
    ort["KARAOGLU_SECRET_KEY"] = "test-anahtari-hizli-test"
    ort["PORT"] = str(PORT)

    proc = subprocess.Popen(
        [str(KOK / ".venv" / "Scripts" / "python.exe"), str(KOK / "wsgi.py")],
        env=ort, cwd=str(KOK),
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    try:
        # sunucu kalkana kadar bekle
        for _ in range(50):
            try:
                istek("/saglik")
                break
            except Exception:
                time.sleep(0.2)
        else:
            print("SUNUCU KALKMADI")
            sys.exit(1)

        print("\n[1] Saglik + statik")
        d, govde, _ = istek("/saglik")
        kontrol("GET /saglik -> 200 'ok'", d == 200 and govde.startswith("ok"), govde[:20])
        d, _, _ = istek("/static/css/style.css")
        kontrol("CSS servis ediliyor", d == 200)
        d, _, _ = istek("/logo")
        kontrol("Logo uç", d in (200, 302))

        print("\n[2] Ilk kurulum (admin otomatik AKTIF olmali)")
        d, govde, _ = istek("/ilk-kurulum")
        kontrol("Ilk kurulum sayfasi acilir", d == 200)
        d, _, konum = istek("/ilk-kurulum", {"kullanici_adi": "testadmin", "sifre": "1234", "sifre2": "1234"})
        kontrol("Admin olusturuldu (yonlendirme)", d in (302, 303), f"durum={d} konum={konum}")

        print("\n[3] Giris -> admin tam yetki, onizleme modu YOK")
        d, govde, _ = istek("/giris")
        kontrol("Giris sayfasi acilir", d == 200)
        # session cookie ile giris
        import http.cookiejar
        cj = http.cookiejar.CookieJar()
        c_ac = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
        try:
            c_ac.open(urllib.request.Request(BASE + "/giris", data=urllib.parse.urlencode(
                {"kullanici_adi": "testadmin", "sifre": "1234"}).encode()), timeout=10)
        except urllib.error.HTTPError:
            pass  # 302 (panel'e yonlendirme) bekleniyor
        cookie = "; ".join(f"{c.name}={c.value}" for c in cj)
        ac2 = urllib.request.build_opener()
        ac2.addheaders = [("Cookie", cookie)]
        d, govde, _ = istek("/")  # cookie'siz istek girise donmeli
        kontrol("Cookie'siz '/' girise yonlenir", d in (301, 302), f"durum={d}")
        req = urllib.request.Request(BASE + "/", headers={"Cookie": cookie})
        r2 = ac2.open(req, timeout=10)
        panel = r2.read().decode("utf-8", "replace")
        kontrol("Panel acilir (Hesap Tablosu)", r2.status == 200 and "HESAP TABLOSU" in panel.upper())
        kontrol("ONIZLEME MODU bandi YOK (admin aktif)", "ÖNİZLEME MODU" not in panel)

        print("\n[4] Alim fiisi kaydi + listeleme")
        veri = {"uretici_ad": "Test Satici", "uretici_tel": "0555", "cesit": "Trilya",
                "tarih": "2026-09-29", "odeme_durumu": "Ödendi", "kg_No:1": "10", "fiyat_No:1": "50"}
        req = urllib.request.Request(BASE + "/alim", data=urllib.parse.urlencode(veri).encode(),
                                     headers={"Cookie": cookie})
        r3 = ac2.open(req, timeout=10)
        kontrol("Alim fiisi kaydedildi (PDF'e yonlenir)", "alim-fis" in r3.url or r3.status == 200, r3.url)
        req = urllib.request.Request(BASE + "/saticilar", headers={"Cookie": cookie})
        r4 = ac2.open(req, timeout=10)
        kontrol("Satici listede gorunuyor", "Test Satici" in r4.read().decode("utf-8", "replace"))

        print("\n[5] Gider + teslimat")
        req = urllib.request.Request(BASE + "/giderler", data=urllib.parse.urlencode(
            {"tarih": "2026-09-29", "tur": "Yakıt", "tutar": "100", "aciklama": "test"}).encode(),
            headers={"Cookie": cookie})
        r5 = ac2.open(req, timeout=10)
        kontrol("Gider kaydedildi", r5.status == 200)
        req = urllib.request.Request(BASE + "/teslimatlar", data=urllib.parse.urlencode(
            {"firma_adi": "Test Firma", "tarih": "2026-09-29", "alinan": "0", "kg_No:1": "5", "fiyat_No:1": "60"}).encode(),
            headers={"Cookie": cookie})
        r6 = ac2.open(req, timeout=10)
        kontrol("Teslimat kaydedildi", r6.status == 200)

        print("\n[6] Rapor + yedek + kullanici paneli")
        req = urllib.request.Request(BASE + "/raporlar", headers={"Cookie": cookie})
        kontrol("Raporlar acilir", ac2.open(req, timeout=10).status == 200)
        req = urllib.request.Request(BASE + "/yedek", headers={"Cookie": cookie})
        r7 = ac2.open(req, timeout=10)
        kontrol("Yedek indirilir (SQLite dosyasi)", r7.status == 200 and b"SQLite" in r7.read(100))
        req = urllib.request.Request(BASE + "/kullanicilar", headers={"Cookie": cookie})
        govde8 = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Kullanicilar sayfasinda admin AKTIF", "AKTİF" in govde8 and "testadmin" in govde8)
        kontrol("Sifre Goster butonu KALDIRILDI", "sifreGoster" not in govde8)

        print("\n[7] Guvenlik: sifre hash-only + brute-force kilit")
        import sqlite3
        db_yolu = Path(gecici) / "zeytin_takip.db"
        c = sqlite3.connect(db_yolu)
        satir = c.execute("SELECT sifre_gizli FROM kullanicilar WHERE kullanici_adi='testadmin'").fetchone()
        c.close()
        kontrol("DB'de geri cozulebilir sifre YOK", satir is not None and satir[0] is None)
        # 5 hatali deneme (yavas hash + kilit sayaci)
        for _ in range(5):
            try:
                c_ac.open(urllib.request.Request(BASE + "/giris", data=urllib.parse.urlencode(
                    {"kullanici_adi": "testadmin", "sifre": "yanlissifre"}).encode()), timeout=10)
            except Exception:
                pass
        # 6. istek: DOGRU sifre — kilit aktifse REDDEDILMELI (302 donmez)
        import http.client
        conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=10)
        conn.request("POST", "/giris", urllib.parse.urlencode(
            {"kullanici_adi": "testadmin", "sifre": "1234"}))
        r8 = conn.getresponse()
        kontrol("Kilit suresinde dogru sifre bile reddedilir (302 yok)", r8.status != 302, f"durum={r8.status}")
        r8.read()

        print("\n[8] Ayarlar: logo uretim + secim, fiyat/komisyon formu KALDIRILDI")
        req = urllib.request.Request(BASE + "/ayarlar", headers={"Cookie": cookie})
        govde9 = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Ayarlar sayfasinda 5 logo varyanti var",
                govde9.count("/logo-sec") >= 5 and govde9.count("SEÇ") >= 5)
        kontrol("Kalibre fiyat formu KALDIRILDI", "fiyat_No:1" not in govde9)
        kontrol("Komisyon formu KALDIRILDI", "KOMİSYON MİKTARI" not in govde9)
        kontrol("Onerilen logo isaretli", "★ önerilen" in govde9)
        # logo sec
        req = urllib.request.Request(BASE + "/logo-sec", data=urllib.parse.urlencode(
            {"stil": "Altin"}).encode(), headers={"Cookie": cookie})
        r9 = ac2.open(req, timeout=10)
        kontrol("Logo secimi kaydedildi", r9.status == 200)
        # /logo artik SVG donduruyor mu?
        req = urllib.request.Request(BASE + "/logo", headers={"Cookie": cookie})
        r10 = ac2.open(req, timeout=10)
        govde10 = r10.read().decode("utf-8", "replace")
        kontrol("/logo SVG uretti (Altin stil)",
                r10.headers.get("Content-Type", "").startswith("image/svg") and "svg" in govde10,
                r10.headers.get("Content-Type", ""))

        print("\n[9] Firma adi degisince logo YENIDEN uretilir (stil korunur)")
        # yeni firma adi kaydet
        req = urllib.request.Request(BASE + "/ayarlar", data=urllib.parse.urlencode(
            {"firma_adi": "Zeytin AŞ"}).encode(), headers={"Cookie": cookie})
        r11 = ac2.open(req, timeout=10)
        kontrol("Firma adi kaydedildi", r11.status == 200)
        # secili stil (Altin) korunmali ve monogram ZA olmali
        req = urllib.request.Request(BASE + "/logo", headers={"Cookie": cookie})
        r12 = ac2.open(req, timeout=10)
        svg12 = r12.read().decode("utf-8", "replace")
        kontrol("Logo yeni ada gore uretildi (ZA monogram)", ">ZA<" in svg12, svg12[100:200])
        kontrol("Secili stil korundu (Altin renkleri)", "#B8860B" in svg12)
        # eski ada don
        req = urllib.request.Request(BASE + "/ayarlar", data=urllib.parse.urlencode(
            {"firma_adi": "KARAOĞLU"}).encode(), headers={"Cookie": cookie})
        ac2.open(req, timeout=10)

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()

    print("\n[10] DATA_DIR fallback: /data yazilamazsa varsayilana dusme")
    eski_env = os.environ.get("KARAOGLU_DATA_DIR")
    os.environ["KARAOGLU_DATA_DIR"] = gecici
    import webdb
    kontrol("Yazilabilir KARAOGLU_DATA_DIR kullanilir",
            webdb._hazirla_data_dir() == Path(gecici))
    engel = Path(gecici) / "engel.txt"
    engel.write_text("x", encoding="utf-8")
    os.environ["KARAOGLU_DATA_DIR"] = str(engel / "alt")
    dusulen = webdb._hazirla_data_dir()
    kontrol("Yazilamaz yolda varsayilan data/ klasorune dusuldu",
            dusulen == webdb.BASE_DIR / "data", str(dusulen))
    if eski_env is None:
        os.environ.pop("KARAOGLU_DATA_DIR", None)
    else:
        os.environ["KARAOGLU_DATA_DIR"] = eski_env
    # testin olusturdugu bos klasoru temizle (projenin gercek data'sina dokunma)
    try:
        if dusulen == webdb.BASE_DIR / "data" and dusulen.exists() \
                and not any(dusulen.iterdir()):
            dusulen.rmdir()
    except OSError:
        pass

    print("\n" + "=" * 50)
    print(f"SONUC: {len(PASS)} basarili, {len(FAIL)} hatali")
    if FAIL:
        print("HATALAR:", *FAIL, sep="\n  - ")
        sys.exit(1)
    print("TUM TESTLER GECTI ✔")


if __name__ == "__main__":
    main()
