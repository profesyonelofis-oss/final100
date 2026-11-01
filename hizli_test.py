# -*- coding: utf-8 -*-
"""KARAOGLU Zeytin Takip - hizli duman testi.

Yerel sunucuyu GECICI veritabaniyla ayaga kaldirir, tum ana akisi
saniyeler icinde test eder ve kapanir. Yerel data/ klasorune dokunmaz.

Calistirma:
    Windows : .venv\\Scripts\\python.exe hizli_test.py
    Linux   : .venv/bin/python hizli_test.py

Sanal ortam yoksa test kendi yorumlayicisiyla (sys.executable) sunucuyu baslatir.
"""
import http.client
import http.cookiejar
import os
import re
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


def python_yolu():
    """Sunucuyu baslatacak yorumlayici: varsa proje ici sanal ortam, yoksa bu Python.

    Windows'ta .venv\\Scripts\\python.exe, Linux/macOS'ta .venv/bin/python.
    """
    for aday in (KOK / ".venv" / "Scripts" / "python.exe", KOK / ".venv" / "bin" / "python"):
        if aday.exists():
            return str(aday)
    return sys.executable


PASS, FAIL = [], []


def kontrol(ad, kosul, detay=""):
    (PASS if kosul else FAIL).append(ad + (f"  [{detay}]" if detay and not kosul else ""))
    print(("  OK   " if kosul else "  HATA ") + ad + (f"  ({detay})" if detay and not kosul else ""))


_anon_jar = http.cookiejar.CookieJar()
JETON_RE = re.compile(r'name="csrf_token" value="([^"]+)"')


def jeton(govde):
    """Sayfadaki gizli CSRF jetonunu dondurur (yoksa bos metin)."""
    eslesme = JETON_RE.search(govde)
    return eslesme.group(1) if eslesme else ""


def istek(yol, veri=None):
    """GET/POST atar; (durum, govde, son_url) dondurur.

    Cerezler korunur: CSRF jetonu ayni oturuma ait olmak zorunda.
    """
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *a, **k):
            return None
    ac = urllib.request.build_opener(
        NoRedirect, urllib.request.HTTPCookieProcessor(_anon_jar))
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
        [python_yolu(), str(KOK / "wsgi.py")],
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
        anon_jeton = jeton(govde)
        kontrol("Ilk kurulum formunda CSRF jetonu var", bool(anon_jeton))
        d, govde, _ = istek("/ilk-kurulum", {"kullanici_adi": "testadmin", "sifre": "1234",
                                             "sifre2": "1234", "csrf_token": anon_jeton})
        kontrol("Zayif sifre ilk kurulumda reddedilir",
                d == 200 and "en az 8" in govde, f"durum={d}")
        d, _, konum = istek("/ilk-kurulum", {"kullanici_adi": "testadmin",
                                             "sifre": "GucluSifre2026",
                                             "sifre2": "GucluSifre2026",
                                             "csrf_token": anon_jeton})
        kontrol("Admin olusturuldu (yonlendirme)", d in (302, 303), f"durum={d} konum={konum}")

        print("\n[3] Giris -> admin tam yetki, onizleme modu YOK")
        d, govde, _ = istek("/giris")
        kontrol("Giris sayfasi acilir", d == 200)
        # session cookie ile giris (CSRF jetonu da bu oturuma bagli)
        cj = http.cookiejar.CookieJar()
        c_ac = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
        giris_govde = c_ac.open(BASE + "/giris", timeout=10).read().decode("utf-8", "replace")
        giris_jeton = jeton(giris_govde)
        kontrol("Giris formunda CSRF jetonu var", bool(giris_jeton))
        try:
            c_ac.open(urllib.request.Request(BASE + "/giris", data=urllib.parse.urlencode(
                {"kullanici_adi": "testadmin", "sifre": "GucluSifre2026",
                 "csrf_token": giris_jeton}).encode()), timeout=10)
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
        admin_jeton = jeton(panel)
        kontrol("Panelde CSRF jetonu var", bool(admin_jeton))

        print("\n[4] Alim fiisi kaydi + listeleme (CSRF korumasi dahil)")
        # CSRF: jetonsuz POST islenmemeli
        sahte = {"uretici_ad": "CSRF Test Satici", "uretici_tel": "0555",
                 "cesit": "Trilya", "tarih": "2026-09-29", "odeme_durumu": "Ödendi",
                 "kg_No:1": "1", "fiyat_No:1": "1"}
        req = urllib.request.Request(BASE + "/alim", data=urllib.parse.urlencode(sahte).encode(),
                                     headers={"Cookie": cookie})
        r_csrf = ac2.open(req, timeout=10)
        r_csrf.read()
        req = urllib.request.Request(BASE + "/saticilar", headers={"Cookie": cookie})
        satici_liste = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("CSRF: jetonsuz alim kaydi OLUSMAZ",
                "CSRF Test Satici" not in satici_liste)
        veri = {"uretici_ad": "Test Satici", "uretici_tel": "0555", "cesit": "Trilya",
                "tarih": "2026-09-29", "odeme_durumu": "Ödendi", "kg_No:1": "10",
                "fiyat_No:1": "50", "csrf_token": admin_jeton}
        req = urllib.request.Request(BASE + "/alim", data=urllib.parse.urlencode(veri).encode(),
                                     headers={"Cookie": cookie})
        r3 = ac2.open(req, timeout=10)
        kontrol("Alim fiisi kaydedildi (fis sayfasina yonlenir)",
                "alim-fis" in r3.url, r3.url)
        # musteriye verilecek fis ciktisi kontrolu
        fis_yolu = r3.url.split("/alim-fis/")[1].split("?")[0] if "alim-fis" in r3.url else "1"
        req = urllib.request.Request(BASE + "/alim-fis/" + fis_yolu, headers={"Cookie": cookie})
        rf = ac2.open(req, timeout=10)
        fis_govde = rf.read().decode("utf-8", "replace")
        kontrol("Fis sayfasinda yazdir butonu var", "YAZDIR / PDF KAYDET" in fis_govde)
        kontrol("Fis musteri bilgisi (satici) iceriyor", "Test Satici" in fis_govde)
        kontrol("Fis tarih formati gun.ay.yil", "29.09.2026" in fis_govde)
        kontrol("Fis imza alanlari var (musteriye verilecek)",
                "İmza (Satıcı)" in fis_govde and "İmza (Alıcı)" in fis_govde)
        req = urllib.request.Request(BASE + "/saticilar", headers={"Cookie": cookie})
        r4 = ac2.open(req, timeout=10)
        kontrol("Satici listede gorunuyor", "Test Satici" in r4.read().decode("utf-8", "replace"))

        print("\n[5] Gider + teslimat")
        req = urllib.request.Request(BASE + "/giderler", data=urllib.parse.urlencode(
            {"tarih": "2026-09-29", "tur": "Yakıt", "tutar": "100", "aciklama": "test",
             "csrf_token": admin_jeton}).encode(),
            headers={"Cookie": cookie})
        r5 = ac2.open(req, timeout=10)
        kontrol("Gider kaydedildi", r5.status == 200)
        req = urllib.request.Request(BASE + "/teslimatlar", data=urllib.parse.urlencode(
            {"firma_adi": "Test Firma", "tarih": "2026-09-29", "alinan": "0",
             "kg_No:1": "5", "fiyat_No:1": "60", "csrf_token": admin_jeton}).encode(),
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

        print("\n[7] Guvenlik: sifre hash-only + giris reddi")
        import sqlite3
        db_yolu = Path(gecici) / "zeytin_takip.db"
        c = sqlite3.connect(db_yolu)
        satir = c.execute("SELECT sifre_gizli FROM kullanicilar WHERE kullanici_adi='testadmin'").fetchone()
        c.close()
        kontrol("DB'de geri cozulebilir sifre YOK", satir is not None and satir[0] is None)
        # Ham istemci: yonlendirme takip edilmeden gercek durum kodu gorulur.
        # (Eski test Content-Type gondermedigi icin sifre hic okunmuyordu.)
        def ham_giris(sifre, jeton, ad="testadmin"):
            baglanti = http.client.HTTPConnection("127.0.0.1", PORT, timeout=10)
            baglanti.request("POST", "/giris", urllib.parse.urlencode(
                {"kullanici_adi": ad, "sifre": sifre, "csrf_token": jeton}),
                {"Cookie": cookie,
                 "Content-Type": "application/x-www-form-urlencoded"})
            cevap = baglanti.getresponse()
            govde = cevap.read().decode("utf-8", "replace")
            baglanti.close()
            return cevap.status, govde

        d_yanlis, govde_yanlis = ham_giris("yanlissifre", giris_jeton)
        kontrol("Yanlis sifre giris yapmaz (302 yok, hata mesaji)",
                d_yanlis != 302 and "şifre hatalı" in govde_yanlis.lower(),
                f"durum={d_yanlis}")
        d_dogru, _ = ham_giris("GucluSifre2026", giris_jeton)
        kontrol("Dogru sifre giris yapar (302)", d_dogru in (302, 303), f"durum={d_dogru}")
        # Kaba kuvvet kilidi: 5 hatali denemeden sonra dogru sifre de reddedilir
        def deneme_satiri():
            bag = sqlite3.connect(db_yolu)
            try:
                return bag.execute(
                    "SELECT hatali, kilit_bitis FROM giris_denemeleri "
                    "WHERE kullanici_adi='testadmin'").fetchone()
            finally:
                bag.close()

        kontrol("Basarili giris deneme sayacini sifirlar", deneme_satiri() is None)
        for _ in range(5):
            ham_giris("yanlissifre", giris_jeton)
        satir_kilit = deneme_satiri()
        kontrol("5 hatali denemeden sonra DB'de kilit kaydi var",
                satir_kilit is not None and satir_kilit[0] >= 5 and bool(satir_kilit[1]),
                str(satir_kilit))
        d_kilit, govde_kilit = ham_giris("GucluSifre2026", giris_jeton)
        kontrol("Kilitliyken dogru sifre giris YAPMAZ (302 yok)",
                d_kilit != 302, f"durum={d_kilit}")
        kontrol("Kilit mesaji gosterilir", "çok fazla" in govde_kilit.lower())
        # kilit suresini gecmise cek: giris yeniden serbest olmali
        bag = sqlite3.connect(db_yolu)
        bag.execute("UPDATE giris_denemeleri SET kilit_bitis='2000-01-01T00:00:00' "
                    "WHERE kullanici_adi='testadmin'")
        bag.commit()
        bag.close()
        d_serbest, _ = ham_giris("GucluSifre2026", giris_jeton)
        kontrol("Kilit suresi dolunca giris yapilir (302)", d_serbest in (302, 303),
                f"durum={d_serbest}")
        kontrol("Kilit kaydi temizlendi", deneme_satiri() is None)

        # Kilit anahtari kullanici adi + IP: baska ad kilitten etkilenmemeli
        for _ in range(5):
            ham_giris("yanlissifre", giris_jeton, "yokboylead")
        d_baska, _ = ham_giris("GucluSifre2026", giris_jeton)
        kontrol("Baska kullanici adindaki kilit testadmin'i ETKILEMEZ",
                d_baska in (302, 303), f"durum={d_baska}")

        # CSRF: jetonsuz giris denemesi oturum ACMAMALI
        anon_cj = http.cookiejar.CookieJar()
        anon_ac = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(anon_cj))
        anon_govde = anon_ac.open(BASE + "/giris", timeout=10).read().decode("utf-8", "replace")
        kontrol("Anonim oturumda da CSRF jetonu uretilir", bool(jeton(anon_govde)))
        baglanti = http.client.HTTPConnection("127.0.0.1", PORT, timeout=10)
        baglanti.request("POST", "/giris", urllib.parse.urlencode(
            {"kullanici_adi": "testadmin", "sifre": "GucluSifre2026"}),
            {"Cookie": "; ".join(f"{c.name}={c.value}" for c in anon_cj),
             "Content-Type": "application/x-www-form-urlencoded"})
        r8 = baglanti.getresponse()
        r8.read()
        baglanti.close()

        class _YonlendirmeYok(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *a, **k):
                return None

        anon_kontrol = urllib.request.build_opener(
            _YonlendirmeYok, urllib.request.HTTPCookieProcessor(anon_cj))
        try:
            d_panel = anon_kontrol.open(BASE + "/", timeout=10).status
        except urllib.error.HTTPError as hata:
            d_panel = hata.code
        kontrol("CSRF: jetonsuz giris oturum ACMAZ (panele girilmez)",
                d_panel in (302, 401, 403), f"durum={d_panel}")

        print("\n[8] Ayarlar: otomatik logo (3 stil), fiyat/komisyon formu KALDIRILDI")
        req = urllib.request.Request(BASE + "/ayarlar", headers={"Cookie": cookie})
        govde9 = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Ayarlar sayfasinda 3 otomatik logo stili var",
                govde9.count("/logo-otomatik?stil=") >= 3 and govde9.count("SEÇ") >= 3)
        kontrol("Kalibre fiyat formu KALDIRILDI", "fiyat_No:1" not in govde9)
        kontrol("Komisyon formu KALDIRILDI", "KOMİSYON MİKTARI" not in govde9)
        kontrol("Kaldirilan /logo-sec baglantisi YOK", "/logo-sec" not in govde9)
        # gecersiz stil reddedilmeli: kayit degismemeli, SVG de uretilmemeli
        req = urllib.request.Request(BASE + "/logo-otomatik-sec", data=urllib.parse.urlencode(
            {"stil": "yokboylestil", "csrf_token": admin_jeton}).encode(), headers={"Cookie": cookie})
        r9h = ac2.open(req, timeout=10)
        r9h.read()
        req = urllib.request.Request(BASE + "/ayarlar", headers={"Cookie": cookie})
        govde9h = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Gecersiz logo stili reddedilir (secim degismez)",
                r9h.status == 200 and "oto-logo-kart secili" not in govde9h)
        req = urllib.request.Request(BASE + "/logo-otomatik?stil=yokboylestil",
                                     headers={"Cookie": cookie})
        r9s = ac2.open(req, timeout=10)
        kontrol("Gecersiz stil icin SVG uretilmez",
                not r9s.headers.get("Content-Type", "").startswith("image/svg"),
                r9s.headers.get("Content-Type", ""))
        # stil sec (POST /ayarlar'a yonlendirir, kart secili gorunur)
        req = urllib.request.Request(BASE + "/logo-otomatik-sec", data=urllib.parse.urlencode(
            {"stil": "altin", "csrf_token": admin_jeton}).encode(), headers={"Cookie": cookie})
        r9 = ac2.open(req, timeout=10)
        govde9b = r9.read().decode("utf-8", "replace")
        kontrol("Logo secimi kaydedildi (kart secili)",
                r9.status == 200 and "oto-logo-kart secili" in govde9b
                and "SEÇİLİ" in govde9b)
        # otomatik logo SVG uretimi
        req = urllib.request.Request(BASE + "/logo-otomatik?stil=altin",
                                     headers={"Cookie": cookie})
        r10 = ac2.open(req, timeout=10)
        govde10 = r10.read().decode("utf-8", "replace")
        kontrol("/logo-otomatik SVG uretti (Altin stil)",
                r10.headers.get("Content-Type", "").startswith("image/svg")
                and "<svg" in govde10 and "#B08D1E" in govde10,
                r10.headers.get("Content-Type", ""))
        # panel (kenar cubugu) secili otomatik logoyu kullaniyor mu?
        req = urllib.request.Request(BASE + "/", headers={"Cookie": cookie})
        panel9 = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Panel secili otomatik logoyu kullaniyor",
                "/logo-otomatik?stil=altin" in panel9)

        print("\n[9] Firma adi degisince logo YENIDEN uretilir (stil korunur)")
        # yeni firma adi kaydet
        req = urllib.request.Request(BASE + "/ayarlar", data=urllib.parse.urlencode(
            {"firma_adi": "Zeytin AŞ", "csrf_token": admin_jeton}).encode(), headers={"Cookie": cookie})
        r11 = ac2.open(req, timeout=10)
        kontrol("Firma adi kaydedildi", r11.status == 200)
        # secili stil (altin) korunmali ve monogram ZA olmali
        req = urllib.request.Request(BASE + "/logo-otomatik?stil=altin",
                                     headers={"Cookie": cookie})
        r12 = ac2.open(req, timeout=10)
        svg12 = r12.read().decode("utf-8", "replace")
        kontrol("Logo yeni ada gore uretildi (ZA monogram)", ">ZA<" in svg12, svg12[100:200])
        kontrol("Secili stil korundu (Altin renkleri)", "#B08D1E" in svg12)
        req = urllib.request.Request(BASE + "/ayarlar", headers={"Cookie": cookie})
        govde12 = ac2.open(req, timeout=10).read().decode("utf-8", "replace")
        kontrol("Ayarlarda Altin stili hala secili",
                "oto-logo-kart secili" in govde12 and "SEÇİLİ" in govde12)
        # eski ada don
        req = urllib.request.Request(BASE + "/ayarlar", data=urllib.parse.urlencode(
            {"firma_adi": "KARAOĞLU", "csrf_token": admin_jeton}).encode(), headers={"Cookie": cookie})
        ac2.open(req, timeout=10)

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()

    print("\n[10] DATA_DIR: KARAOGLU_DATA_DIR veritabani klasorunu belirler")
    kod = "import webdb; print(webdb.DATA_DIR)"
    hedef = tempfile.mkdtemp(prefix="zeytin_datadir_")
    ozel = subprocess.run(
        [sys.executable, "-c", kod], cwd=str(KOK),
        env={**os.environ, "KARAOGLU_DATA_DIR": hedef},
        capture_output=True, text=True)
    kontrol("KARAOGLU_DATA_DIR verilince o klasor kullanilir",
            Path(ozel.stdout.strip()).resolve() == Path(hedef).resolve(),
            ozel.stdout.strip() + ozel.stderr.strip())
    vardi = (KOK / "data").exists()
    temiz_ort = {k: v for k, v in os.environ.items() if k != "KARAOGLU_DATA_DIR"}
    varsayilan = subprocess.run(
        [sys.executable, "-c", kod], cwd=str(KOK), env=temiz_ort,
        capture_output=True, text=True)
    kontrol("Degisken yoksa proje ici data/ kullanilir",
            Path(varsayilan.stdout.strip()).resolve() == (KOK / "data").resolve(),
            varsayilan.stdout.strip() + varsayilan.stderr.strip())

    print("\n[11] Giris kilidi ayarlari ortam degiskeninden okunur")
    kilit_kod = "import webdb; print(webdb.GIRIS_AZAMI_HATA, webdb.GIRIS_KILIT_DAKIKA)"
    ayar_ort = {**temiz_ort, "KARAOGLU_DATA_DIR": hedef}
    ozel_ayar = subprocess.run(
        [sys.executable, "-c", kilit_kod], cwd=str(KOK),
        env={**ayar_ort, "KARAOGLU_GIRIS_AZAMI_HATA": "3",
              "KARAOGLU_GIRIS_KILIT_DAKIKA": "30"},
        capture_output=True, text=True)
    kontrol("Ortam degiskenleri kilidi ayarlar (3 / 30)",
            ozel_ayar.stdout.split() == ["3", "30"],
            ozel_ayar.stdout.strip() + ozel_ayar.stderr.strip())
    varsayilan_ayar = subprocess.run(
        [sys.executable, "-c", kilit_kod], cwd=str(KOK), env=ayar_ort,
        capture_output=True, text=True)
    kontrol("Degisken yoksa varsayilanlar (5 / 15)",
            varsayilan_ayar.stdout.split() == ["5", "15"],
            varsayilan_ayar.stdout.strip() + varsayilan_ayar.stderr.strip())
    gecersiz_ayar = subprocess.run(
        [sys.executable, "-c", kilit_kod], cwd=str(KOK),
        env={**ayar_ort, "KARAOGLU_GIRIS_AZAMI_HATA": "abc",
              "KARAOGLU_GIRIS_KILIT_DAKIKA": "99999"},
        capture_output=True, text=True)
    kontrol("Gecersiz/aralik disi deger varsayilana doner (5 / 15)",
            gecersiz_ayar.stdout.split() == ["5", "15"],
            gecersiz_ayar.stdout.strip() + gecersiz_ayar.stderr.strip())

    # testin olusturdugu bos klasoru temizle (projenin gercek data'sina dokunma)
    try:
        if not vardi and (KOK / "data").exists() and not any((KOK / "data").iterdir()):
            (KOK / "data").rmdir()
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
