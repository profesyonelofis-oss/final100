# -*- coding: utf-8 -*-
"""KARAOĞLU Zeytin Takip - WEB surumu (Flask).

Calistirma:
    python app.py            -> http://127.0.0.1:5000
Yayinlama icin README.md dosyasina bakiniz (wsgi.py + gunicorn/waitress).
"""

import io
import os
from pathlib import Path
from datetime import datetime, timedelta
from functools import wraps

from flask import (Flask, Response, flash, redirect, render_template, request,
                   send_file, session, url_for)

import webdb
import webpdf

app = Flask(__name__)


def _gizli_anahtar():
    """SECRET_KEY: env var ise onu kullan; yoksa data/ icinde kalici rastgele anahtar."""
    env = os.environ.get("KARAOGLU_SECRET_KEY")
    if env:
        return env
    yol = webdb.DATA_DIR / "secret_key"
    try:
        if yol.exists():
            deger = yol.read_text(encoding="utf-8").strip()
            if deger:
                return deger
        import secrets as _secrets
        deger = _secrets.token_hex(32)
        webdb.DATA_DIR.mkdir(parents=True, exist_ok=True)
        yol.write_text(deger, encoding="utf-8")
        try:
            os.chmod(yol, 0o600)
        except OSError:
            pass
        return deger
    except OSError:
        return "karaoglu-zeytin-takip-yedek-anahtar"


app.secret_key = _gizli_anahtar()

YETKI_GEREKTIRMEZ = {"giris", "ilk_kurulum", "cikis", "static"}


def giris_sadece(f):
    """Yalnizca oturum kontrolu; lisans kapisi OLMADAN."""
    @wraps(f)
    def sarmal(*args, **kwargs):
        if not session.get("kullanici"):
            return redirect(url_for("giris"))
        if webdb.get_user(session["kullanici"]) is None:
            session.clear()
            return redirect(url_for("giris"))
        return f(*args, **kwargs)
    return sarmal


def giris_gerekli(f):
    """Oturum + lisans kapisi.

    Lisansi aktif olmayan kullanici sayfalari GEZEBILIR (onizleme modu);
    yalnizca YAZMA istekleri (POST) engellenir ve uyelik sayfasina yonlendirilir.
    """
    @wraps(f)
    def sarmal(*args, **kwargs):
        if not session.get("kullanici"):
            return redirect(url_for("giris"))
        u = webdb.get_user(session["kullanici"])
        if u is None:
            session.clear()
            return redirect(url_for("giris"))
        if u["rol"] != "admin":
            # sure dolan (deneme veya ucretli) uyelik otomatik kilitlenir
            if webdb.lisans_suresi_dolmus_mu(u["bitis"]):
                yeni_durum = "deneme_bitti" if u["lisans"] == "aktif" and (
                    (datetime.now() - datetime.strptime(u["bitis"], "%Y-%m-%d")
                     ).days <= 2) else "suresi_bitti"
                webdb.set_lisans(u["ad"], yeni_durum)
                u = webdb.get_user(session["kullanici"])
            if u["lisans"] != "aktif" and request.method == "POST":
                if u["lisans"] == "deneme_bitti":
                    flash("2 günlük deneme süreniz doldu. Ödeme yapıp üye "
                          "adınızı bildirin; yönetici onayladıktan sonra "
                          "sistem tekrar açılır.", "uyari")
                else:
                    flash("Üyeliğiniz onay bekliyor; kayıt ekleyemezsiniz. "
                          "Ödeme sonrası tüm işlemler açılır.", "uyari")
                return redirect(url_for("uyelik"))
        return f(*args, **kwargs)
    return sarmal


@app.template_filter("tarih_tr")
def tarih_tr(deger):
    """2026-09-29 -> 29.09.2026 (fis ciktisi icin)."""
    try:
        y, a, g = str(deger).split("-")
        return f"{g}.{a}.{y}"
    except Exception:
        return deger


@app.context_processor
def sabitler():
    _user = None
    if session.get("kullanici"):
        _user = webdb.get_user(session["kullanici"])
    _logo_dosyasi = webdb.get_logo_dosyasi()
    _oto_stil = webdb.get_oto_logo_stil()
    # Cache-kirici: yuklu logoda surum zamanini, otomatikte firma adinin
    # kisa ozetini (hash) kullan (ad degisince logo da tazelenir).
    _firma = webdb.get_firma_adi()
    _v_oto = str(abs(hash(_firma)) % 100000)
    return {
        "USER": _user,
        "FIRMA_ADI": _firma,
        "LOGO_URL": ("/logo?v=%d" % (webdb.get_logo_mtime() or 0))
                    if _logo_dosyasi
                    else ("/logo-otomatik?stil=%s&v=%s"
                          % (_oto_stil, _v_oto)
                          if _oto_stil
                          else url_for("static", filename="logo_default.png")),
        "fmt_tr": webdb.fmt_tr,
        "KALIBRELER": webdb.KALIBRELER,
        "CESITLER": webdb.CESITLER,
        "GIDER_TURLERI": webdb.GIDER_TURLERI,
        "bugun": datetime.now().strftime("%Y-%m-%d"),
        "simdi": datetime.now(),
        "aylar": ["", "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran",
                  "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"],
    }


# ---------------------------------------------------------------------------
# Giris / kurulum
# ---------------------------------------------------------------------------

@app.route("/giris", methods=["GET", "POST"])
def giris():
    webdb.init_db()
    if request.method == "POST":
        ad = request.form.get("kullanici_adi", "").strip()
        sifre = request.form.get("sifre", "")
        if webdb.check_user(ad, sifre):
            session["kullanici"] = ad
            u = webdb.get_user(ad)
            if u["rol"] != "admin" and webdb.lisans_suresi_dolmus_mu(u["bitis"]):
                webdb.set_lisans(ad, "suresi_bitti")
            return redirect(url_for("genel_bakis"))
        flash("Kullanıcı adı veya şifre hatalı.", "hata")
    return render_template("giris.html", kullanici_var=webdb.has_any_user())


@app.route("/ilk-kurulum", methods=["GET", "POST"])
def ilk_kurulum():
    webdb.init_db()
    if webdb.has_any_user():
        return redirect(url_for("giris"))
    if request.method == "POST":
        ad = request.form.get("kullanici_adi", "").strip()
        s1 = request.form.get("sifre", "")
        s2 = request.form.get("sifre2", "")
        if not ad or len(s1) < 4:
            flash("Kullanıcı adı girin, şifre en az 4 karakter olsun.", "hata")
        elif s1 != s2:
            flash("Şifreler birbiriyle uyuşmuyor.", "hata")
        else:
            webdb.create_user(ad, s1)
            # ilk hesap otomatik olarak yonetici (admin) olur
            webdb.set_rol(ad, "admin")
            flash("Kurulum tamamlandı, giriş yapabilirsiniz.", "basari")
            return redirect(url_for("giris"))
    return render_template("ilk_kurulum.html")


@app.route("/kayit", methods=["GET", "POST"])
def kayit():
    webdb.init_db()
    if request.method == "POST":
        ad = request.form.get("kullanici_adi", "").strip()
        s1 = request.form.get("sifre", "")
        s2 = request.form.get("sifre2", "")
        cihaz_id = request.form.get("cihaz_id", "").strip()
        ip = request.remote_addr or ""
        mac = _mac_adresi_cikar()
        if not ad or len(s1) < 4:
            flash("Kullan\u0131c\u0131 ad\u0131 girin, \u015fifre en az 4 karakter olsun.", "hata")
        elif s1 != s2:
            flash("\u015eifreler birbiriyle uyu\u015fmuyor.", "hata")
        elif not cihaz_id:
            flash("Cihaz do\u011frulamas\u0131 al\u0131namad\u0131; taray\u0131c\u0131n\u0131z\u0131 yenileyin.", "hata")
        elif webdb.cihaz_uyelik_var_mi(cihaz_id) and not webdb.cihaz_izni_var_mi(cihaz_id):
            onceki = webdb.cihaz_adi_getir(cihaz_id)
            flash("Bu cihazdan daha \u00f6nce \u00fcyelik a\u00e7\u0131lm\u0131\u015f (%s). "
                  "Tekrar \u00fcyelik a\u00e7amazs\u0131n\u0131z; \u00f6deme yapt\u0131ktan sonra "
                  "y\u00f6neticiyle ileti\u015fime ge\u00e7in." % (onceki or "bilinmeyen"), "hata")
        elif not webdb.create_user(ad, s1, cihaz_id=cihaz_id,
                                   mac_adresi=mac, ip_adresi=ip):
            flash("Bu kullan\u0131c\u0131 ad\u0131 daha \u00f6nce al\u0131nm\u0131\u015f.", "hata")
        elif webdb.cihaz_izni_var_mi(cihaz_id):
            # Yonetici bu cihazin engelini kaldirmis: 6 aylik tam uyelik.
            # Izin tek kullanimlik: uye acilinca silinir (ayni cihazdan
            # tekrar uyelik acilamaz).
            webdb.cihaz_izni_sil(cihaz_id)
            flash("Kayd\u0131n\u0131z al\u0131nd\u0131! Y\u00f6netici izniyle "
                  "6 ayl\u0131k tam \u00fcyeli\u011finiz ba\u015flad\u0131; t\u00fcm sistemi "
                  "\u00fccretsiz kullanabilirsiniz.", "basari")
            return redirect(url_for("giris"))
        else:
            flash("Kayd\u0131n\u0131z al\u0131nd\u0131! 2 g\u00fcnl\u00fck deneme s\u00fcr\u00fcminiz ba\u015flad\u0131; "
                  "t\u00fcm sistemi kullanabilirsiniz. Deneme bitince \u00fcyelik sayfas\u0131ndan "
                  "\u00f6deme yapabilirsiniz.", "basari")
            return redirect(url_for("giris"))
    return render_template("kayit.html")


def _mac_adresi_cikar():
    """Sunucunun MAC adresini dondurur (Render konteynerinde ayni MAC cikar;
    gercek cihaz ayrimi tarayicidan gelen cihaz_id ile yapilir)."""
    try:
        import uuid
        return uuid.getnode().to_bytes(6, "big").hex(":")
    except Exception:
        return ""


@app.route("/cikis")
def cikis():
    session.clear()
    return redirect(url_for("giris"))


# ---------------------------------------------------------------------------
# Saglik kontrolu (Render health check)
# ---------------------------------------------------------------------------

@app.route("/saglik")
def saglik():
    """Render ucretsiz planda uyku/uyanma davranisi icin saglik ucu.

    Veritabanini hazirlar ve 200 dondurur; Render "Application Loading"
    ekranindan kurtulmak icin servisin yant verdigini dogrular.
    """
    webdb.init_db()
    return Response("ok", mimetype="text/plain")


# ---------------------------------------------------------------------------
# Genel bakis
# ---------------------------------------------------------------------------

@app.route("/")
@giris_gerekli
def genel_bakis():
    webdb.init_db()
    stats = webdb.get_summary_stats()
    notlar = webdb.get_teslimat_notlari()
    fisler_son = webdb.get_all_alim_fisleri()[:8]
    satislar_son = webdb.get_all_satis_fisleri()[:8]
    return render_template("genel_bakis.html", s=stats, notlar=notlar,
                           fisler_son=fisler_son, satislar_son=satislar_son)


@app.route("/uyelik", methods=["GET", "POST"])
@giris_sadece
def uyelik():
    webdb.init_db()
    u = webdb.get_user(session["kullanici"])
    if request.method == "POST" and request.form.get("islem") == "odeme-bildir":
        mesaj = request.form.get("bildirim_mesaj", "").strip()
        if not mesaj:
            flash("Lütfen ödeme bilgisi (havale saati, tutar, son 4 hane vb.) yazın.", "hata")
        else:
            webdb.odeme_bildirimi_ekle(u["ad"], mesaj)
            flash("Ödeme bildiriminiz yöneticiye iletildi. Onaylandıktan "
                  "sonra sistem açılacaktır.", "basari")
        return redirect(url_for("uyelik"))
    return render_template("uyelik.html", u=u,
                           ucret=webdb.get_ayar("uyelik_ucret"),
                           iban=webdb.get_ayar("uyelik_iban"),
                           havale_ad=webdb.get_ayar("uyelik_havale_ad"),
                           not_bilgi=webdb.get_ayar("uyelik_not"))


@app.route("/teslimat-notlari", methods=["POST"])
@giris_gerekli
def teslimat_notlari_kaydet():
    if not _admin_mi():
        flash("Teslimat notlarını yalnızca yönetici düzenleyebilir.", "hata")
        return redirect(url_for("genel_bakis"))
    webdb.update_teslimat_notlari(
        request.form.get("kg", ""), request.form.get("para", ""))
    flash("Teslimat notları kaydedildi.", "basari")
    return redirect(url_for("genel_bakis"))


# ---------------------------------------------------------------------------
# Alim fisi (satici girisi)
# ---------------------------------------------------------------------------

@app.route("/alim", methods=["GET", "POST"])
@giris_gerekli
def alim():
    webdb.init_db()
    if request.method == "POST":
        ad = request.form.get("uretici_ad", "").strip()
        tel = request.form.get("uretici_tel", "").strip()
        cesit = request.form.get("cesit", "").strip()
        tarih = request.form.get("tarih") or datetime.now().strftime("%Y-%m-%d")
        odenen = webdb._tr_to_float(request.form.get("odenen", "0"))
        odeme_durumu = request.form.get("odeme_durumu", "")
        if not ad:
            flash("Satıcı ad soyad girin.", "hata")
        elif not tel:
            flash("Satıcı telefonu girin.", "hata")
        elif not cesit:
            flash("Zeytin çeşidini seçin.", "hata")
        elif odeme_durumu not in ("Ödendi", "Ödenmedi"):
            flash("Ödeme durumunu seçin.", "hata")
        else:
            detaylar = []
            for kalibre in webdb.KALIBRELER:
                kg = webdb._tr_to_float(request.form.get(f"kg_{kalibre}", ""), 0.0)
                fiyat = webdb._tr_to_float(request.form.get(f"fiyat_{kalibre}", ""), 0.0)
                if kg > 0:
                    detaylar.append({"kalibre": kalibre, "zeytin_cesidi": cesit,
                                     "brut": kg, "dara": 0.0, "net": kg, "fiyat": fiyat})
            if not detaylar:
                flash("En az bir zeytin numarasına geçerli KG girin.", "hata")
            else:
                toplam = sum(d["net"] * d["fiyat"] for d in detaylar)
                if toplam <= 0:
                    flash("Hesaplanan tutar 0 olamaz.", "hata")
                else:
                    if odeme_durumu == "Ödendi" and odenen <= 0:
                        odenen = toplam
                    fis_id = webdb.add_alim_fisi(ad, tel, tarih, odenen, detaylar)
                    flash(f"Alım fişi #{fis_id} kaydedildi.", "basari")
                    return redirect(url_for("alim_fis_pdf", fis_id=fis_id))
    fisler = webdb.get_all_alim_fisleri()
    fiyatlar = webdb.get_kalibre_fiyatlari()
    komisyon = webdb.get_komisyon()
    return render_template("alim.html", fisler=fisler, fiyatlar=fiyatlar,
                           komisyon=komisyon)


@app.route("/alim-fis/<int:fis_id>")
@giris_gerekli
def alim_fis_pdf(fis_id):
    fis = next((f for f in webdb.get_all_alim_fisleri() if f[0] == fis_id), None)
    if not fis:
        flash("Fiş bulunamadı.", "hata")
        return redirect(url_for("saticilar"))
    detay = webdb.get_alim_detay(fis_id)
    # Tarayici yazdirma motoru kullan: her dilde kusursuz, PDF olarak kaydet
    # secenegi tarayicinin yazdir diyalogunda zaten mevcut.
    return render_template("fis.html", fis=fis, detay=detay)


@app.route("/alim-sil/<int:fis_id>", methods=["POST"])
@giris_gerekli
def alim_sil(fis_id):
    webdb.delete_alim_fisi(fis_id)
    flash(f"Alım fişi #{fis_id} silindi.", "basari")
    return redirect(request.referrer or url_for("saticilar"))


# ---------------------------------------------------------------------------
# Saticilar
# ---------------------------------------------------------------------------

@app.route("/saticilar")
@giris_gerekli
def saticilar():
    webdb.init_db()
    bakiyeler = webdb.get_satici_bakiyeleri()
    harfler = sorted({s["ad"][:1].upper() for s in bakiyeler if s["ad"]})
    harf = request.args.get("harf", "").strip().upper()
    if harf:
        bakiyeler = [s for s in bakiyeler
                     if s["ad"][:1].upper() == harf]
    return render_template("saticilar.html", saticilar=bakiyeler, harfler=harfler,
                           harf=harf)


@app.route("/satici/<ad>")
@giris_gerekli
def satici_detay(ad):
    fisler = webdb.get_alim_fisleri_by_uretici(ad)
    odemeler = webdb.get_odemeler_by_uretici(ad)
    # alim_fisi kolonlari: 0=id, 1=ad, 2=tel, 3=tarih, 4=kg, 5=tutar, 6=odenen, 7=fark
    toplam_tutar = sum(f[5] for f in fisler)
    toplam_odenen = sum(f[6] for f in fisler) + sum(o[1] for o in odemeler)
    return render_template("satici_detay.html", ad=ad, fisler=fisler,
                           odemeler=odemeler, toplam_tutar=toplam_tutar,
                           toplam_odenen=toplam_odenen,
                           kalan=toplam_tutar - toplam_odenen)


@app.route("/satici/<ad>/odeme", methods=["POST"])
@giris_gerekli
def satici_odeme(ad):
    tutar = request.form.get("tutar", "")
    if webdb._tr_to_float(tutar) > 0:
        webdb.add_odeme(ad, request.form.get("tarih") or
                        datetime.now().strftime("%Y-%m-%d"), tutar,
                        request.form.get("aciklama", ""))
        flash("Ödeme kaydedildi.", "basari")
    else:
        flash("Geçerli bir tutar girin.", "hata")
    return redirect(url_for("satici_detay", ad=ad))


@app.route("/satici-sil/<ad>", methods=["POST"])
@giris_gerekli
def satici_sil(ad):
    webdb.delete_satici(ad)
    flash(f"{ad} ve tüm fişleri silindi.", "basari")
    return redirect(url_for("saticilar"))


# ---------------------------------------------------------------------------
# Teslimatlar (satis fisleri)
# ---------------------------------------------------------------------------

@app.route("/teslimatlar", methods=["GET", "POST"])
@giris_gerekli
def teslimatlar():
    webdb.init_db()
    if request.method == "POST":
        firma = request.form.get("firma_adi", "").strip()
        tarih = request.form.get("tarih") or datetime.now().strftime("%Y-%m-%d")
        alinan = webdb._tr_to_float(request.form.get("alinan", "0"))
        detaylar = []
        for kalibre in webdb.KALIBRELER:
            kg = webdb._tr_to_float(request.form.get(f"kg_{kalibre}", ""), 0.0)
            fiyat = webdb._tr_to_float(request.form.get(f"fiyat_{kalibre}", ""), 0.0)
            if kg > 0:
                detaylar.append({"kalibre": kalibre, "kg": kg, "fiyat": fiyat})
        if not firma:
            flash("Firma adı girin.", "hata")
        elif not detaylar:
            flash("En az bir satıra KG girin.", "hata")
        else:
            fis_id = webdb.add_satis_fisi(firma, tarih, alinan, detaylar)
            flash(f"Teslimat fişi #{fis_id} kaydedildi.", "basari")
            return redirect(url_for("teslimatlar"))
    fisler = webdb.get_all_satis_fisleri()
    fiyatlar = webdb.get_kalibre_fiyatlari()
    return render_template("teslimatlar.html", fisler=fisler, fiyatlar=fiyatlar)


@app.route("/teslimat-sil/<int:fis_id>", methods=["POST"])
@giris_gerekli
def teslimat_sil(fis_id):
    webdb.delete_satis_fisi(fis_id)
    flash(f"Teslimat fişi #{fis_id} silindi.", "basari")
    return redirect(url_for("teslimatlar"))


# ---------------------------------------------------------------------------
# Giderler
# ---------------------------------------------------------------------------

@app.route("/giderler", methods=["GET", "POST"])
@giris_gerekli
def giderler():
    webdb.init_db()
    if request.method == "POST":
        tur = request.form.get("tur", "").strip()
        tutar = request.form.get("tutar", "")
        if not tur or webdb._tr_to_float(tutar) <= 0:
            flash("Gider türü ve geçerli tutar girin.", "hata")
        else:
            webdb.add_gider(request.form.get("tarih") or
                            datetime.now().strftime("%Y-%m-%d"),
                            tur, tutar, request.form.get("aciklama", ""))
            flash("Gider kaydedildi.", "basari")
            return redirect(url_for("giderler"))
    return render_template("giderler.html", giderler=webdb.get_all_giderler())


@app.route("/gider-sil/<int:gider_id>", methods=["POST"])
@giris_gerekli
def gider_sil(gider_id):
    webdb.delete_gider(gider_id)
    flash("Gider silindi.", "basari")
    return redirect(url_for("giderler"))


# ---------------------------------------------------------------------------
# Ayarlar (fiyat + komisyon)
# ---------------------------------------------------------------------------

@app.route("/ayarlar", methods=["GET", "POST"])
@giris_gerekli
def ayarlar():
    webdb.init_db()
    if request.method == "POST":
        for kalibre in webdb.KALIBRELER:
            if f"fiyat_{kalibre}" in request.form:
                webdb.update_kalibre_fiyat(kalibre,
                                           request.form[f"fiyat_{kalibre}"])
        if "komisyon" in request.form:
            webdb.set_komisyon(request.form["komisyon"])
        if "firma_adi" in request.form:
            webdb.set_firma_adi(request.form["firma_adi"])
        if "oto_logo_stil" in request.form:
            try:
                webdb.set_oto_logo_stil(request.form["oto_logo_stil"])
            except ValueError as e:
                flash(str(e), "hata")
        flash("Ayarlar kaydedildi.", "basari")
        return redirect(url_for("ayarlar"))
    return render_template("ayarlar.html", fiyatlar=webdb.get_kalibre_fiyatlari(),
                           komisyon=webdb.get_komisyon(),
                           firma_adi=webdb.get_firma_adi(),
                           logo_yuklu=webdb.logo_var_mi(),
                           oto_stil=webdb.get_oto_logo_stil(),
                           oto_stiller=webdb.OTO_LOGO_STILLER)


# ---------------------------------------------------------------------------
# Raporlar
# ---------------------------------------------------------------------------

@app.route("/raporlar")
@giris_gerekli
def raporlar():
    webdb.init_db()
    fisler = webdb.get_all_alim_fisleri()
    satislar = webdb.get_all_satis_fisleri()
    giderler = webdb.get_all_giderler()
    stats = webdb.get_summary_stats()
    return render_template("raporlar.html", fisler=fisler, satislar=satislar,
                           giderler=giderler, s=stats)


# ---------------------------------------------------------------------------
# Yedek indirme (DB)
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Logo
# ---------------------------------------------------------------------------

_LOGO_MIMELER = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg",
                 ".gif": "image/gif", ".webp": "image/webp"}


@app.route("/logo")
def logo():
    """Yuklenen logoyu servis eder; yoksa varsayilan logoya yonlenir."""
    dosya = webdb.get_logo_dosyasi()
    if not dosya:
        return redirect(url_for("static", filename="logo_default.png"))
    yol = webdb.DATA_DIR / dosya
    resp = send_file(yol, mimetype=_LOGO_MIMELER[Path(dosya).suffix.lower()])
    resp.headers["Cache-Control"] = "public, max-age=3600"
    return resp


@app.route("/logo-yukle", methods=["POST"])
@giris_gerekli
def logo_yukle():
    dosya = request.files.get("logo")
    if not dosya or not dosya.filename:
        flash("Lütfen bir resim dosyası seçin.", "hata")
        return redirect(url_for("ayarlar"))
    uzanti = Path(dosya.filename).suffix.lower()
    try:
        webdb.set_logo(dosya.read(), uzanti)
        flash("Logo güncellendi.", "basari")
    except ValueError as e:
        flash(str(e), "hata")
    return redirect(url_for("ayarlar"))


@app.route("/logo-sifirla", methods=["POST"])
@giris_gerekli
def logo_sifirla():
    webdb.reset_logo()
    flash("Logo varsayılana döndürüldü.", "basari")
    return redirect(url_for("ayarlar"))


# ---------------------------------------------------------------------------
# Otomatik logo (firma adindan uretilir; dosya yuklemeye gerek yok)
# ---------------------------------------------------------------------------

@app.route("/logo-otomatik")
def logo_otomatik():
    """Otomatik uretilen SVG logoyu servis eder. Query: stil, ad (onizleme), v."""
    stil = request.args.get("stil") or "klasik"
    ad = request.args.get("ad") or webdb.get_firma_adi()
    svg = webdb.oto_logo_svg(stil, ad)
    if svg is None:
        return redirect(url_for("static", filename="logo_default.png"))
    resp = Response(svg, mimetype="image/svg+xml")
    resp.headers["Cache-Control"] = "public, max-age=3600"
    return resp


@app.route("/logo-otomatik-sec", methods=["POST"])
@giris_gerekli
def logo_otomatik_sec():
    """Otomatik uretilen logolari aktif/pasif yapar.
    stil bos gonderilirse otomatik logo kapatilir."""
    try:
        webdb.set_oto_logo_stil(request.form.get("stil", ""))
        if request.form.get("stil", "").strip():
            flash("Otomatik logo seçildi.", "basari")
        else:
            flash("Otomatik logo kapatıldı.", "basari")
    except ValueError as e:
        flash(str(e), "hata")
    return redirect(url_for("ayarlar"))


# ---------------------------------------------------------------------------
# Kullanicilar (admin)
# ---------------------------------------------------------------------------

def _admin_mi():
    u = webdb.get_user(session.get("kullanici") or "")
    return bool(u) and u["rol"] == "admin"


@app.route("/kullanicilar")
@giris_gerekli
def kullanicilar():
    if not _admin_mi():
        flash("Bu b\u00f6l\u00fcm yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    sifreler = {}
    for k in webdb.get_all_users():
        sifreler[k[1]] = webdb.get_user_sifre(k[1]) or "-"
    # Kullanici -> cihaz eslesmesi
    cihazlar = {c[1]: c for c in webdb.get_cihaz_id_listesi()}
    bildirimler = webdb.get_odeme_bildirimleri()
    okunmamis = webdb.okunmamis_bildirim_sayisi()
    return render_template("kullanicilar.html",
                           kullanicilar=webdb.get_all_users(),
                           sifreler=sifreler,
                           cihazlar=cihazlar,
                           bildirimler=bildirimler,
                           okunmamis=okunmamis,
                           ucret=webdb.get_ayar("uyelik_ucret"),
                           iban=webdb.get_ayar("uyelik_iban"),
                           havale_ad=webdb.get_ayar("uyelik_havale_ad"),
                           not_bilgi=webdb.get_ayar("uyelik_not"))


@app.route("/bildirim/<int:bid>/okundu", methods=["POST"])
@giris_gerekli
def bildirim_okundu(bid):
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    webdb.bildirim_okundu_yap(bid)
    flash("Bildirim okundu olarak i\u015faretlendi.", "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/cihaz/<cihaz_id>/engel-kaldir", methods=["POST"])
@giris_gerekli
def cihaz_engel_kaldir(cihaz_id):
    """Yonetici onayli cihaz engeli kaldirma (ayni cihaz tekrar uyelik acabilir)."""
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    webdb.cihaz_engeli_kaldir(cihaz_id)
    flash("Cihaz engeli kald\u0131r\u0131ld\u0131; bu cihazdan a\u00e7\u0131lacak yeni "
          "\u00fcyelik otomatik 6 ayl\u0131k tam \u00fcyelik olur.", "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/kullanici/<ad>/onayla", methods=["POST"])
@giris_gerekli
def kullanici_onayla(ad):
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    try:
        ay = max(1, min(36, int(request.form.get("ay", "12"))))
    except (TypeError, ValueError):
        ay = 12
    bitis = (datetime.now() + timedelta(days=30 * ay)).strftime("%Y-%m-%d")
    webdb.set_lisans(ad, "aktif", bitis)
    flash("%s kullan\u0131c\u0131s\u0131n\u0131n \u00fcyeli\u011fi %d ay a\u00e7\u0131ld\u0131." % (ad, ay), "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/kullanici/<ad>/durdur", methods=["POST"])
@giris_gerekli
def kullanici_durdur(ad):
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    webdb.set_lisans(ad, "reddedildi")
    flash("%s kullan\u0131c\u0131s\u0131n\u0131n \u00fcyeli\u011fi durduruldu." % ad, "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/kullanici/<ad>/sifre", methods=["POST"])
@giris_gerekli
def kullanici_sifre(ad):
    if not _admin_mi():
        flash("Bu işlem yalnızca yönetici içindir.", "hata")
        return redirect(url_for("genel_bakis"))
    s1 = request.form.get("sifre", "")
    s2 = request.form.get("sifre2", "")
    if not webdb.get_user(ad):
        flash("Kullanıcı bulunamadı.", "hata")
    elif len(s1) < 4:
        flash("Şifre en az 4 karakter olmalı.", "hata")
    elif s1 != s2:
        flash("Şifreler uyuşmuyor.", "hata")
    else:
        webdb.set_user_sifre(ad, s1)
        flash("%s kullanıcısının şifresi güncellendi." % ad, "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/kullanici/<ad>/sil", methods=["POST"])
@giris_gerekli
def kullanici_sil(ad):
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    if ad == session.get("kullanici"):
        flash("Kendi hesab\u0131n\u0131z\u0131 silemezsiniz.", "hata")
    else:
        webdb.delete_user(ad)
        flash("%s kullan\u0131c\u0131s\u0131 silindi." % ad, "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/uyelik-ayarla", methods=["POST"])
@giris_gerekli
def uyelik_ayarla():
    if not _admin_mi():
        flash("Bu i\u015flem yaln\u0131zca y\u00f6netici i\u00e7indir.", "hata")
        return redirect(url_for("genel_bakis"))
    webdb.set_ayar("uyelik_ucret", request.form.get("ucret", ""))
    webdb.set_ayar("uyelik_iban", request.form.get("iban", ""))
    webdb.set_ayar("uyelik_havale_ad", request.form.get("havale_ad", ""))
    webdb.set_ayar("uyelik_not", request.form.get("not_bilgi", ""))
    flash("\u00dcyelik bilgileri kaydedildi.", "basari")
    return redirect(url_for("kullanicilar"))


@app.route("/yedek")
@giris_gerekli
def yedek():
    if not _admin_mi():
        flash("Yedek indirme yalnızca yönetici içindir.", "hata")
        return redirect(url_for("genel_bakis"))
    with open(webdb.DB_PATH, "rb") as f:
        veri = f.read()
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    return Response(veri, mimetype="application/octet-stream",
                    headers={"Content-Disposition":
                             f"attachment; filename=zeytin_takip_yedek_{stamp}.db"})


# ---------------------------------------------------------------------------
# Otomatik yedek (bilgisayardaki zamanlanmis gorev icin)
# ---------------------------------------------------------------------------

def _yedek_anahtari_dogru_mu():
    """Otomatik yedek isteginin anahtarini kontrol eder.
    Anahtar: data/ klasorundeki secret_key (KARAOGLU_SECRET_KEY env'i de kabul)."""
    verilen = (request.args.get("anahtar") or
               request.headers.get("X-Yedek-Anahtar") or "")
    beklenen = os.environ.get("KARAOGLU_SECRET_KEY")
    if not beklenen:
        try:
            beklenen = (webdb.DATA_DIR / "secret_key").read_text(
                encoding="utf-8").strip()
        except OSError:
            beklenen = ""
    return bool(beklenen) and bool(verilen) and verilen == beklenen


@app.route("/yedek-otomatik")
def yedek_otomatik():
    """Gizli anahtarla cagrilan otomatik yedek ucu.

    Render silinmelerine karsi bilgisayardaki zamanlanmis gorev her gun
    bu adrese baglanip veritabanini indirir. Anahtar bilmeyen indiremez.
    Kullanim:
      /yedek-otomatik?anahtar=<SECRET_KEY>
    Yani t:
      ?durum  -> son yedek zamanini dondurur (anahtar gerekli)
    """
    if request.args.get("durum"):
        if not _yedek_anahtari_dogru_mu():
            return Response("Yetkisiz", status=403)
        try:
            stamp = (webdb.DATA_DIR / "son_otomatik_yedek.txt").read_text(
                encoding="utf-8").strip()
        except OSError:
            stamp = ""
        return Response(stamp or "henuz-yedek-alinmadi", mimetype="text/plain")
    if not _yedek_anahtari_dogru_mu():
        return Response("Yetkisiz", status=403)
    # Yedekten hemen once once DB'yi guvenli şekilde diskte sabitle
    try:
        conn = webdb.get_connection()
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        conn.close()
    except Exception:
        pass
    with open(webdb.DB_PATH, "rb") as f:
        veri = f.read()
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    try:
        (webdb.DATA_DIR / "son_otomatik_yedek.txt").write_text(
            stamp, encoding="utf-8")
    except OSError:
        pass
    return Response(veri, mimetype="application/octet-stream",
                    headers={"Content-Disposition":
                             f"attachment; filename=zeytin_takip_yedek_{stamp}.db"})


if __name__ == "__main__":
    webdb.init_db()
    port = int(os.environ.get("PORT", 5000))
    app.run(host="0.0.0.0", port=port, debug=False)
