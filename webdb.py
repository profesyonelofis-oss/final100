# -*- coding: utf-8 -*-
"""KARAOĞLU Zeytin Takip - WEB surumu veri katmani.

Masaustu uygulamasindaki database.py ile ayni tablolari kullanir; ayrica:
  - satis_fisi.firma_adi (masaustundeki gec kolon, web'de kullanilir)
  - odemeler tablosu satici odeme kayitlari icin
  - kullanicilar tablosu web girisi (sifre hash) icin
Veritabani konumu: <klasor>/data/zeytin_takip.db (web klasorunun icinde tasinar)
"""

import hashlib
import os
import base64
import secrets
import sqlite3
from datetime import datetime, timedelta
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent
DATA_DIR = Path(os.environ.get("KARAOGLU_DATA_DIR")
                or (BASE_DIR / "data"))
DATA_DIR.mkdir(parents=True, exist_ok=True)
DB_PATH = DATA_DIR / "zeytin_takip.db"

SCHEMA_VERSION = 2
KALIBRELER = ["Duble", "No:1", "No:2", "No:3", "No:4", "Yağlık"]
CESITLER = ["Trilya", "Edremit", "Tekir", "Domat", "Uslu"]
GIDER_TURLERI = ["Yakıt", "Bakım", "Muhasebe", "Elektrik", "Su", "Vergi",
                 "İşçilik", "Nakliye", "Diğer"]


DEFAULT_FIRMA_ADI = "KARAOĞLU"  # Ayarlar > Firma Adi'ndan degistirilir


def get_connection():
    conn = sqlite3.connect(DB_PATH)
    conn.execute("PRAGMA foreign_keys = ON")
    return conn


# ---------------------------------------------------------------------------
# Kurulum ve goc
# ---------------------------------------------------------------------------

def _create_schema(cursor):
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS alimlar (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uretici_ad TEXT NOT NULL,
            uretici_tel TEXT,
            tarih TEXT NOT NULL,
            kalibre TEXT NOT NULL,
            brut_kilo REAL NOT NULL,
            dara REAL NOT NULL,
            net_kilo REAL NOT NULL,
            fiyat_kg REAL NOT NULL,
            toplam_tutar REAL NOT NULL,
            zeytin_cesidi TEXT
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS giderler (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tarih TEXT NOT NULL,
            tur TEXT NOT NULL,
            tutar REAL NOT NULL,
            aciklama TEXT
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS kalibre_fiyatlari (
            kalibre TEXT PRIMARY KEY,
            fiyat REAL NOT NULL
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS odemeler (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uretici_ad TEXT NOT NULL,
            tarih TEXT NOT NULL,
            tutar REAL NOT NULL,
            aciklama TEXT
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS satis_fisi (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            firma_adi TEXT DEFAULT '',
            tarih TEXT NOT NULL,
            toplam_kg REAL NOT NULL,
            hesaplanan_tutar REAL NOT NULL,
            alinan_para REAL NOT NULL,
            fark REAL NOT NULL
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS satis_detay (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            fis_id INTEGER NOT NULL,
            kalibre TEXT NOT NULL,
            kg REAL NOT NULL,
            birim_fiyat REAL NOT NULL,
            tutar REAL NOT NULL,
            FOREIGN KEY(fis_id) REFERENCES satis_fisi(id) ON DELETE CASCADE
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS alim_fisi (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uretici_ad TEXT NOT NULL,
            uretici_tel TEXT,
            tarih TEXT NOT NULL,
            toplam_kg REAL NOT NULL,
            hesaplanan_tutar REAL NOT NULL,
            odenen_para REAL NOT NULL,
            fark REAL NOT NULL
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS alim_detay (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            fis_id INTEGER NOT NULL,
            zeytin_cesidi TEXT NOT NULL,
            kalibre TEXT NOT NULL,
            brut_kilo REAL NOT NULL,
            dara REAL NOT NULL,
            net_kilo REAL NOT NULL,
            birim_fiyat REAL NOT NULL,
            tutar REAL NOT NULL,
            FOREIGN KEY(fis_id) REFERENCES alim_fisi(id) ON DELETE CASCADE
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS ayarlar (
            id INTEGER PRIMARY KEY,
            komisyon_kg REAL NOT NULL DEFAULT 0.0
        )""")
    if cursor.execute("SELECT COUNT(*) FROM ayarlar").fetchone()[0] == 0:
        cursor.execute("INSERT INTO ayarlar (id, komisyon_kg) VALUES (1, 0.0)")

    cursor.execute("""
        CREATE TABLE IF NOT EXISTS teslimat_notlari (
            id INTEGER PRIMARY KEY, kg TEXT, para TEXT
        )""")
    if cursor.execute("SELECT COUNT(*) FROM teslimat_notlari").fetchone()[0] == 0:
        cursor.execute("INSERT INTO teslimat_notlari (id, kg, para) VALUES (1, '', '')")

    if cursor.execute("SELECT COUNT(*) FROM kalibre_fiyatlari").fetchone()[0] == 0:
        cursor.executemany(
            "INSERT INTO kalibre_fiyatlari (kalibre, fiyat) VALUES (?, 0)",
            [(k,) for k in KALIBRELER])

    cursor.execute("""
        CREATE TABLE IF NOT EXISTS genel_ayarlar (
            anahtar TEXT PRIMARY KEY,
            deger TEXT
        )""")

    cursor.execute("""
        CREATE TABLE IF NOT EXISTS kullanicilar (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kullanici_adi TEXT UNIQUE NOT NULL,
            sifre_hash TEXT NOT NULL,
            sifre_gizli TEXT,
            rol TEXT DEFAULT 'kullanici',
            lisans_durumu TEXT DEFAULT 'beklemede',
            lisans_bitis TEXT,
            olusturma TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )""")

    cursor.execute("""
        CREATE TABLE IF NOT EXISTS cihaz_kayitlari (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            cihaz_id TEXT UNIQUE NOT NULL,
            kullanici_adi TEXT NOT NULL,
            mac_adresi TEXT,
            ip_adresi TEXT,
            kayit_zamani TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS odeme_bildirimleri (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kullanici_adi TEXT NOT NULL,
            mesaj TEXT,
            okundu INTEGER DEFAULT 0,
            zaman TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )""")
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS cihaz_izinleri (
            cihaz_id TEXT PRIMARY KEY,
            izin_zamani TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )""")
    # Gecis: bitisi deneme suresinin (3 gun) cok ucunda olan aktif hesaplar
    # ucretli uyeliktir (admin onaylamis / izinle acilmis) — otomatik isaretle
    try:
        cursor.execute(
            "UPDATE kullanicilar SET uyelik_tipi = 'ucretli' "
            "WHERE uyelik_tipi = 'deneme' AND rol != 'admin' "
            "AND lisans_durumu = 'aktif' AND lisans_bitis IS NOT NULL "
            "AND julianday(lisans_bitis) - julianday('now') > 5")
    except sqlite3.OperationalError:
        pass

    # eski DB'lerde kolonlar yoksa ekle
    for kolon_sql in ("ALTER TABLE kullanicilar ADD COLUMN rol TEXT DEFAULT 'kullanici'",
                      "ALTER TABLE kullanicilar ADD COLUMN lisans_durumu TEXT DEFAULT 'beklemede'",
                      "ALTER TABLE kullanicilar ADD COLUMN lisans_bitis TEXT",
                      "ALTER TABLE kullanicilar ADD COLUMN sifre_gizli TEXT",
                      "ALTER TABLE kullanicilar ADD COLUMN cihaz_id TEXT",
                      "ALTER TABLE kullanicilar ADD COLUMN deneme_bitti TEXT DEFAULT '0'",
                      "ALTER TABLE kullanicilar ADD COLUMN uyelik_tipi TEXT DEFAULT 'deneme'"):
        try:
            cursor.execute(kolon_sql)
        except sqlite3.OperationalError:
            pass

    cursor.execute("CREATE INDEX IF NOT EXISTS idx_alim_fisi_tarih ON alim_fisi(tarih)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_alim_fisi_uretici ON alim_fisi(uretici_ad)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_satis_fisi_tarih ON satis_fisi(tarih)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_giderler_tarih ON giderler(tarih)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_alim_detay_fis ON alim_detay(fis_id)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_satis_detay_fis ON satis_detay(fis_id)")


def init_db():
    conn = get_connection()
    cursor = conn.cursor()
    try:
        _create_schema(cursor)
        version = int(cursor.execute("PRAGMA user_version").fetchone()[0])
        if version < SCHEMA_VERSION:
            cursor.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        conn.commit()
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# Kullanicilar (web girisi)
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Sifre saklama (GERI CUZULEBILIR): yonetici panelinde goruntulenebilmesi icin.
# Anahtar data/ klasorunde tutulur; klasor hostinge tasinirsa anahtar da tasınır.
# ---------------------------------------------------------------------------

def _sifre_anahtari_yolu():
    return DATA_DIR / "sifre_anahtari.key"


def _sifre_anahtari():
    """Anahtari oku; yoksa uretip data/ icine kaydet."""
    yol = _sifre_anahtari_yolu()
    if yol.exists():
        return yol.read_text(encoding="utf-8").strip()
    anahtar = secrets.token_urlsafe(32)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    yol.write_text(anahtar, encoding="utf-8")
    try:
        os.chmod(yol, 0o600)
    except OSError:
        pass
    return anahtar


def _xor_mask(sifre):
    """Sifreyi anahtarla maskeler; sadece yonetici goruntulemek icin cozulur.
    Gercek guvenlik saglamaz (anahtar ayni klasorde) - ama DB baskasi eline
    gecse bile sifreler acik metin DEGIL, anahtar dosyasi olmadan okunamaz."""
    if not sifre:
        return ""
    anahtar = _sifre_anahtari()
    veri = sifre.encode("utf-8")
    m_anahtar = anahtar.encode("utf-8")
    kutulu = bytes(b ^ m_anahtar[i % len(m_anahtar)] for i, b in enumerate(veri))
    return base64.urlsafe_b64encode(kutulu).decode("ascii")


def _xor_unmask(kutulu):
    if not kutulu:
        return ""
    anahtar = _sifre_anahtari()
    try:
        veri = base64.urlsafe_b64decode(kutulu.encode("ascii"))
    except Exception:
        return ""
    m_anahtar = anahtar.encode("utf-8")
    return bytes(b ^ m_anahtar[i % len(m_anahtar)] for i, b in enumerate(veri)).decode(
        "utf-8", errors="replace")


def get_user_sifre(kullanici_adi):
    """Yonetici paneli icin saklanan sifreyi cozup dondurur; yoksa None."""
    conn = get_connection()
    row = conn.execute(
        "SELECT sifre_gizli FROM kullanicilar WHERE kullanici_adi = ?",
        (kullanici_adi,)).fetchone()
    conn.close()
    if not row or not row[0]:
        return None
    return _xor_unmask(row[0])


def set_user_sifre(kullanici_adi, sifre):
    """Kullanicinin sifresini hem hash hem geri cozulebilir sakla."""
    conn = get_connection()
    conn.execute(
        "UPDATE kullanicilar SET sifre_hash = ?, sifre_gizli = ? "
        "WHERE kullanici_adi = ?",
        (_hash_password(sifre), _xor_mask(sifre), kullanici_adi))
    conn.commit()
    conn.close()


def create_user(kullanici_adi, sifre, cihaz_id=None, mac_adresi=None,
                ip_adresi=None, uyelik_tipi=None):
    """Yeni kullanici: hash + geri cozulebilir saklama + otomatik 3 gun deneme.

    Deneme lisansi: lisans_durumu='aktif', lisans_bitis=bugun+3 gun.
    Cihaz bilgisi ayni zamanda cihaz_kayitlari tablosuna yazilir; ayni cihazdan
    yeni uyelik acilmasi engellenir.

    uyelik_tipi:
      - None  -> cihaz icin yonetici izni varsa 'ucretli' (6 ay), yoksa 'deneme'
      - 'deneme' / 'ucretli' -> zorlanmis tip (dikkatli kullanin)
    """
    conn = get_connection()
    try:
        izinli = cihaz_id and cihaz_izni_var_mi(cihaz_id)
        # Cihaz kilidi: ayni cihaz_id ile onceki uyelik varsa engelle
        # (yonetici izni vermis cihaz haric)
        if cihaz_id and not izinli:
            var = conn.execute(
                "SELECT 1 FROM cihaz_kayitlari WHERE cihaz_id = ?",
                (cihaz_id,)).fetchone()
            if var:
                return False
        if uyelik_tipi is None:
            uyelik_tipi = "ucretli" if izinli else "deneme"
        if uyelik_tipi == "ucretli":
            # Yonetici izniyle acilan uyelik: 6 ay tam erisim, odeme gerekmez
            lisans_bitis = (datetime.now() + timedelta(days=180)).strftime("%Y-%m-%d")
        else:
            # Normal kayit: 3 gunluk deneme
            lisans_bitis = (datetime.now() + timedelta(days=3)).strftime("%Y-%m-%d")
        conn.execute(
            "INSERT INTO kullanicilar (kullanici_adi, sifre_hash, sifre_gizli, "
            "lisans_durumu, lisans_bitis, cihaz_id, uyelik_tipi) "
            "VALUES (?, ?, ?, 'aktif', ?, ?, ?)",
            (kullanici_adi, _hash_password(sifre), _xor_mask(sifre),
             lisans_bitis, cihaz_id, uyelik_tipi))
        if cihaz_id:
            conn.execute(
                "INSERT OR REPLACE INTO cihaz_kayitlari "
                "(cihaz_id, kullanici_adi, mac_adresi, ip_adresi) "
                "VALUES (?, ?, ?, ?)",
                (cihaz_id, kullanici_adi, mac_adresi, ip_adresi))
        conn.commit()
        return True
    except sqlite3.IntegrityError:
        return False
    finally:
        conn.close()


def _hash_password(sifre, salt=None):
    if salt is None:
        salt = secrets.token_hex(16)
    h = hashlib.pbkdf2_hmac("sha256", sifre.encode("utf-8"),
                            bytes.fromhex(salt), 120_000)
    return salt + "$" + h.hex()


def check_user(kullanici_adi, sifre):
    conn = get_connection()
    row = conn.execute(
        "SELECT sifre_hash FROM kullanicilar WHERE kullanici_adi = ?",
        (kullanici_adi,)).fetchone()
    conn.close()
    if not row:
        return False
    salt, _hex = row[0].split("$", 1)
    return secrets.compare_digest(_hash_password(sifre, salt), row[0])


def has_any_user():
    conn = get_connection()
    n = conn.execute("SELECT COUNT(*) FROM kullanicilar").fetchone()[0]
    conn.close()
    return n > 0


def get_user(kullanici_adi):
    conn = get_connection()
    row = conn.execute(
        "SELECT id, kullanici_adi, rol, lisans_durumu, lisans_bitis, uyelik_tipi "
        "FROM kullanicilar WHERE kullanici_adi = ?", (kullanici_adi,)).fetchone()
    conn.close()
    if not row:
        return None
    return {"id": row[0], "ad": row[1], "rol": row[2] or "kullanici",
            "lisans": row[3] or "beklemede", "bitis": row[4],
            "tip": row[5] if len(row) > 4 and row[5] else "deneme"}


def get_all_users():
    conn = get_connection()
    rows = conn.execute(
        "SELECT id, kullanici_adi, rol, lisans_durumu, lisans_bitis, olusturma "
        "FROM kullanicilar ORDER BY id").fetchall()
    conn.close()
    return rows


def set_lisans(kullanici_adi, durum, bitis=None):
    """durum: 'aktif' | 'beklemede' | 'reddedildi' | 'suresi_bitti'"""
    conn = get_connection()
    conn.execute(
        "UPDATE kullanicilar SET lisans_durumu = ?, lisans_bitis = ? "
        "WHERE kullanici_adi = ?", (durum, bitis, kullanici_adi))
    conn.commit()
    conn.close()


def set_uyelik_tipi(kullanici_adi, tip):
    """tip: 'deneme' | 'ucretli' — admin onayiyla ucretliye gecer."""
    conn = get_connection()
    conn.execute("UPDATE kullanicilar SET uyelik_tipi = ? WHERE kullanici_adi = ?",
                 (tip, kullanici_adi))
    conn.commit()
    conn.close()


def set_rol(kullanici_adi, rol):
    conn = get_connection()
    conn.execute("UPDATE kullanicilar SET rol = ? WHERE kullanici_adi = ?",
                 (rol, kullanici_adi))
    conn.commit()
    conn.close()


def delete_user(kullanici_adi):
    conn = get_connection()
    conn.execute("DELETE FROM kullanicilar WHERE kullanici_adi = ?",
                 (kullanici_adi,))
    # Cihaz kaydini da temizle: silinen kullanicinin cihazindan tekrar
    # uyelik acilabilsin (cihaz kilidi kisiye bagli, cihaza degil).
    conn.execute("DELETE FROM cihaz_kayitlari WHERE kullanici_adi = ?",
                 (kullanici_adi,))
    conn.commit()
    conn.close()


def admin_sayisi():
    conn = get_connection()
    n = conn.execute(
        "SELECT COUNT(*) FROM kullanicilar WHERE rol = 'admin'").fetchone()[0]
    conn.close()
    return n


def lisans_suresi_dolmus_mu(bitis):
    """bitis 'YYYY-MM-DD'; dolmussa True."""
    if not bitis:
        return False
    try:
        return datetime.strptime(bitis, "%Y-%m-%d").date() < datetime.now().date()
    except ValueError:
        return False


# ---------------------------------------------------------------------------
# Cihaz kilidi ve odeme bildirimleri
# ---------------------------------------------------------------------------

def cihaz_uyelik_var_mi(cihaz_id):
    """Bu cihazdan daha once uyelik alinmis mi? (3 gun deneme tek seferlik)"""
    if not cihaz_id:
        return False
    conn = get_connection()
    row = conn.execute(
        "SELECT 1 FROM cihaz_kayitlari WHERE cihaz_id = ?", (cihaz_id,)).fetchone()
    conn.close()
    return bool(row)


def cihaz_adi_getir(cihaz_id):
    """Cihazin kayitli kullanici adini dondurur."""
    conn = get_connection()
    row = conn.execute(
        "SELECT kullanici_adi FROM cihaz_kayitlari WHERE cihaz_id = ?",
        (cihaz_id,)).fetchone()
    conn.close()
    return row[0] if row else None


def get_cihaz_id_listesi():
    """Yonetici paneli icin tum cihaz kayitlari."""
    conn = get_connection()
    rows = conn.execute(
        "SELECT cihaz_id, kullanici_adi, mac_adresi, ip_adresi, kayit_zamani "
        "FROM cihaz_kayitlari ORDER BY id DESC").fetchall()
    conn.close()
    return rows


def cihaz_engeli_kaldir(cihaz_id):
    """Yonetici izin verirse cihaz kaydini siler; ayni cihaz tekrar uyelik acabilir.

    Engeli kaldirilan cihaz 'izinli' olarak isaretlenir; bu cihazdan acilan yeni
    uyelik 2 gunluk deneme yerine otomatik 6 aylik tam uyelik olur.
    """
    conn = get_connection()
    conn.execute("DELETE FROM cihaz_kayitlari WHERE cihaz_id = ?", (cihaz_id,))
    conn.execute(
        "INSERT OR REPLACE INTO cihaz_izinleri (cihaz_id) VALUES (?)", (cihaz_id,))
    conn.commit()
    conn.close()


def cihaz_izni_var_mi(cihaz_id):
    """Bu cihaz icin yonetici 'engel kaldirdi' izni var mi?"""
    if not cihaz_id:
        return False
    conn = get_connection()
    row = conn.execute(
        "SELECT 1 FROM cihaz_izinleri WHERE cihaz_id = ?", (cihaz_id,)).fetchone()
    conn.close()
    return bool(row)


def cihaz_izni_sil(cihaz_id):
    """Izin kaydini siler (uye acildiktan sonra 6 aylik uyelik zaten baslamis olur)."""
    conn = get_connection()
    conn.execute("DELETE FROM cihaz_izinleri WHERE cihaz_id = ?", (cihaz_id,))
    conn.commit()
    conn.close()


def odeme_bildirimi_ekle(kullanici_adi, mesaj):
    """Uyenin yoneticiye odeme bildirimi: uye adi + mesaj."""
    conn = get_connection()
    conn.execute(
        "INSERT INTO odeme_bildirimleri (kullanici_adi, mesaj) VALUES (?, ?)",
        (kullanici_adi, mesaj))
    conn.commit()
    conn.close()


def get_odeme_bildirimleri(sadece_okunmamis=False):
    """Yonetici paneli icin bildirim listesi."""
    conn = get_connection()
    if sadece_okunmamis:
        rows = conn.execute(
            "SELECT id, kullanici_adi, mesaj, zaman FROM odeme_bildirimleri "
            "WHERE okundu = 0 ORDER BY id DESC").fetchall()
    else:
        rows = conn.execute(
            "SELECT id, kullanici_adi, mesaj, okundu, zaman "
            "FROM odeme_bildirimleri ORDER BY id DESC").fetchall()
    conn.close()
    return rows


def okunmamis_bildirim_sayisi():
    conn = get_connection()
    n = conn.execute(
        "SELECT COUNT(*) FROM odeme_bildirimleri WHERE okundu = 0").fetchone()[0]
    conn.close()
    return n


def bildirim_okundu_yap(bildirim_id):
    conn = get_connection()
    conn.execute("UPDATE odeme_bildirimleri SET okundu = 1 WHERE id = ?",
                 (bildirim_id,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Yardimcilar
# ---------------------------------------------------------------------------

def _tr_to_float(s, default=0.0):
    try:
        return float(str(s).replace(",", ".").strip() or 0)
    except (TypeError, ValueError):
        return default


def fmt_tr(x, para=True):
    """1234567.8 -> '1.234.567,80' (para) veya '1.234,568' (kg)."""
    if x is None:
        x = 0.0
    s = f"{x:,.2f}" if para else f"{x:,.3f}"
    return s.replace(",", "@").replace(".", ",").replace("@", ".")


# ---------------------------------------------------------------------------
# Ayarlar
# ---------------------------------------------------------------------------

def get_komisyon():
    conn = get_connection()
    row = conn.execute("SELECT komisyon_kg FROM ayarlar WHERE id = 1").fetchone()
    conn.close()
    return row[0] if row else 0.0


def set_komisyon(miktar):
    conn = get_connection()
    conn.execute("UPDATE ayarlar SET komisyon_kg = ? WHERE id = 1",
                 (_tr_to_float(miktar),))
    conn.commit()
    conn.close()


def get_ayar(anahtar, varsayilan=""):
    conn = get_connection()
    row = conn.execute("SELECT deger FROM genel_ayarlar WHERE anahtar = ?",
                       (anahtar,)).fetchone()
    conn.close()
    return row[0] if row and row[0] is not None else varsayilan


def set_ayar(anahtar, deger):
    conn = get_connection()
    conn.execute("INSERT OR REPLACE INTO genel_ayarlar (anahtar, deger) VALUES (?, ?)",
                 (anahtar, str(deger if deger is not None else "")))
    conn.commit()
    conn.close()


def get_firma_adi():
    """Marka/firma adini dondurur (varsayilan: KARAOĞLU).

    Eski surumlerde kaydedilmis 'KAYA AŞ' degeri otomatik olarak
    KARAOĞLU'na tasinir (marka tamamen degistigi icin).
    """
    conn = get_connection()
    row = conn.execute(
        "SELECT deger FROM genel_ayarlar WHERE anahtar = 'firma_adi'"
    ).fetchone()
    if row and row[0]:
        deger = row[0]
        conn.close()
        if deger.strip() in ("KAYA AŞ", "KAYA AŞ".upper()):
            deger = DEFAULT_FIRMA_ADI
            set_firma_adi(deger)
        return deger
    conn.close()
    return DEFAULT_FIRMA_ADI


# ---------------------------------------------------------------------------
# Logo
# ---------------------------------------------------------------------------

LOGO_FILE_NAME = "logo"  # uzanti set_logo ile eklenir (logo.png, logo.jpg ...)
LOGO_STAMP_FILE = "logo_surum.txt"
LOGO_MAX_BOYUT = 2 * 1024 * 1024  # 2 MB
LOGO_IZINLI_UZANTILAR = {".png", ".jpg", ".jpeg", ".gif", ".webp"}


def get_logo_mtime():
    """Logo dosyasinin surum zamanini dondurur (yoksa None).

    Sablonlarda cache-kiran ?v= parametresi olarak kullanilir.
    """
    try:
        return int((DATA_DIR / LOGO_STAMP_FILE).read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        return None


def set_logo(veri, uzanti):
    """Yuklenen logo icerigini kaydeder; surum zamanini gunceller."""
    uzanti = uzanti.lower()
    if uzanti not in LOGO_IZINLI_UZANTILAR:
        raise ValueError("Desteklenmeyen dosya türü")
    if not veri:
        raise ValueError("Boş dosya")
    if len(veri) > LOGO_MAX_BOYUT:
        raise ValueError("Dosya çok büyük (en fazla 2 MB)")
    hedef = DATA_DIR / (LOGO_FILE_NAME + uzanti)
    hedef.write_bytes(veri)
    # eski farkli uzantili logolari temizle
    for eski in DATA_DIR.glob(LOGO_FILE_NAME + ".*"):
        if eski != hedef:
            try:
                eski.unlink()
            except OSError:
                pass
    (DATA_DIR / LOGO_STAMP_FILE).write_text(
        str(int(datetime.now().timestamp())), encoding="utf-8")
    return hedef


def logo_var_mi():
    return bool(get_logo_dosyasi())


def get_logo_dosyasi():
    """Kayitli logo dosya adini dondurur (yoksa None)."""
    for uzanti in LOGO_IZINLI_UZANTILAR:
        f = DATA_DIR / (LOGO_FILE_NAME + uzanti)
        if f.is_file():
            return LOGO_FILE_NAME + uzanti
    return None


def reset_logo():
    """Logoyu varsayilan sembole dondurur."""
    for eski in DATA_DIR.glob(LOGO_FILE_NAME + ".*"):
        try:
            eski.unlink()
        except OSError:
            pass
    try:
        (DATA_DIR / LOGO_STAMP_FILE).unlink()
    except OSError:
        pass


def set_firma_adi(ad):
    ad = (ad or "").strip()
    if not ad:
        ad = DEFAULT_FIRMA_ADI
    conn = get_connection()
    conn.execute(
        "INSERT OR REPLACE INTO genel_ayarlar (anahtar, deger) "
        "VALUES ('firma_adi', ?)", (ad,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Otomatik logo (firma adindan SVG uretici)
# ---------------------------------------------------------------------------

OTO_LOGO_STILLER = {
    "klasik": {"ad": "Klasik", "sekil": "daire", "bg": "#198754",
               "fg": "#FFFFFF", "cizgi": None},
    "koyu":   {"ad": "Koyu",   "sekil": "daire", "bg": "#14532D",
               "fg": "#FFFFFF", "cizgi": None},
    "altin":  {"ad": "Altın",  "sekil": "daire", "bg": "#B08D1E",
               "fg": "#FFFFFF", "cizgi": None},
}
OTO_LOGO_ANAHTARI = "oto_logo_stil"


def get_oto_logo_stil():
    """Secili otomatik logo stil anahtarini dondurur (yoksa/kaldırıldıysa '')."""
    stil = get_ayar(OTO_LOGO_ANAHTARI, "")
    return stil if stil in OTO_LOGO_STILLER else ""


def set_oto_logo_stil(stil):
    """Otomatik logo stilini kaydeder; '' verildiginde secim kaldirilir."""
    stil = (stil or "").strip().lower()
    if stil and stil not in OTO_LOGO_STILLER:
        raise ValueError("Geçersiz logo stili")
    set_ayar(OTO_LOGO_ANAHTARI, stil)


def _oto_logo_parcalari(firma_adi):
    """Firma adindan (bas harfler, alt etiket) uretir.
    'KARAOĞLU' -> ('KA', 'KARAOĞLU'); 'Ali Veli Ltd' -> ('AV', 'ALİ VELİ L')."""
    kelimeler = [k for k in (firma_adi or "").split() if k]
    if not kelimeler:
        kelimeler = [DEFAULT_FIRMA_ADI]
    if len(kelimeler) >= 2:
        bas = (kelimeler[0][0] + kelimeler[1][0]).upper()
    else:
        w = kelimeler[0]
        bas = w[:2].upper() if len(w) >= 2 else w.upper()
    # Turkce buyutme (upper() bazi ortamlarda I->I yapiyor; garantiye alalim)
    ceviri = str.maketrans("abcçdefgğhıijklmnoöprsştuüvyz",
                           "ABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZ")
    # Turkce buyutme (ı->I degil İ, i->I degil I...): once cevir sonra upper
    ceviri = str.maketrans("abcçdefgğhıijklmnoöprsştuüvyz",
                           "ABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZ")
    bas = bas.translate(ceviri).upper()
    alt = " ".join(kelimeler)[:14].translate(ceviri).upper()
    return bas, alt


def oto_logo_svg(stil, firma_adi=None):
    """Otomatik logoyu SVG metni olarak uretir; stil bilinmiyorsa None."""
    s = OTO_LOGO_STILLER.get((stil or "").strip().lower())
    if not s:
        return None
    if firma_adi is None:
        firma_adi = get_firma_adi()
    from xml.sax.saxutils import escape
    bas, alt = _oto_logo_parcalari(firma_adi)
    bas_e, alt_e = escape(bas), escape(alt)
    fs_bas = 36 if len(bas) <= 2 else 28
    fs_alt = 10 if len(alt) <= 12 else 8
    if s["sekil"] == "kare":
        if s["cizgi"]:
            zemin = ('<rect x="6" y="6" width="88" height="88" rx="18" '
                     'fill="%s" stroke="%s" stroke-width="4"/>'
                     % (s["bg"], s["cizgi"]))
        else:
            zemin = ('<rect x="2" y="2" width="96" height="96" rx="18" '
                     'fill="%s"/>' % s["bg"])
    else:
        zemin = '<circle cx="50" cy="50" r="48" fill="%s"/>' % s["bg"]
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">'
        + zemin
        + '<text x="50" y="45" font-family="Arial, Helvetica, sans-serif" '
          'font-size="%d" font-weight="bold" fill="%s" text-anchor="middle" '
          'dominant-baseline="middle">%s</text>'
        % (fs_bas, s["fg"], bas_e)
        + '<text x="50" y="70" font-family="Arial, Helvetica, sans-serif" '
          'font-size="%d" font-weight="bold" letter-spacing="1" fill="%s" '
          'text-anchor="middle">%s</text>'
        % (fs_alt, s["fg"], alt_e)
        + '</svg>')


# ---------------------------------------------------------------------------
# Kalibre fiyat ve isimleri
# ---------------------------------------------------------------------------

def get_kalibre_fiyatlari():
    conn = get_connection()
    rows = conn.execute("SELECT kalibre, fiyat FROM kalibre_fiyatlari").fetchall()
    conn.close()
    d = dict(rows)
    for k in KALIBRELER:
        d.setdefault(k, 0.0)
    return d


def update_kalibre_fiyat(kalibre, fiyat):
    conn = get_connection()
    conn.execute("INSERT OR REPLACE INTO kalibre_fiyatlari (kalibre, fiyat) VALUES (?, ?)",
                 (kalibre, _tr_to_float(fiyat)))
    conn.commit()
    conn.close()


def delete_kalibre(kalibre):
    conn = get_connection()
    conn.execute("DELETE FROM kalibre_fiyatlari WHERE kalibre = ?", (kalibre,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Alim fisleri
# ---------------------------------------------------------------------------

def add_alim_fisi(uretici_ad, uretici_tel, tarih, odenen_para, detaylar):
    """detaylar: [{kalibre, zeytin_cesidi, brut, dara, net, fiyat}]"""
    toplam_kg = sum(d["net"] for d in detaylar)
    toplam_tutar = sum(d["net"] * d["fiyat"] for d in detaylar)
    komisyon = get_komisyon()
    hesaplanan = toplam_tutar - toplam_kg * komisyon
    fark = hesaplanan - odenen_para
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        INSERT INTO alim_fisi (uretici_ad, uretici_tel, tarih, toplam_kg,
                               hesaplanan_tutar, odenen_para, fark)
        VALUES (?, ?, ?, ?, ?, ?, ?)""",
        (uretici_ad, uretici_tel, tarih, toplam_kg, hesaplanan, odenen_para, fark))
    fis_id = cursor.lastrowid
    for d in detaylar:
        cursor.execute("""
            INSERT INTO alim_detay (fis_id, zeytin_cesidi, kalibre, brut_kilo,
                                    dara, net_kilo, birim_fiyat, tutar)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
            (fis_id, d["zeytin_cesidi"], d["kalibre"], d["brut"], d["dara"],
             d["net"], d["fiyat"], d["net"] * d["fiyat"]))
    conn.commit()
    conn.close()
    return fis_id


def update_alim_fisi(fis_id, uretici_ad, uretici_tel, tarih, odenen_para, detaylar):
    toplam_kg = sum(d["net"] for d in detaylar)
    toplam_tutar = sum(d["net"] * d["fiyat"] for d in detaylar)
    komisyon = get_komisyon()
    hesaplanan = toplam_tutar - toplam_kg * komisyon
    fark = hesaplanan - odenen_para
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        UPDATE alim_fisi SET uretici_ad = ?, uretici_tel = ?, tarih = ?,
                             toplam_kg = ?, hesaplanan_tutar = ?,
                             odenen_para = ?, fark = ?
        WHERE id = ?""",
        (uretici_ad, uretici_tel, tarih, toplam_kg, hesaplanan, odenen_para,
         fark, fis_id))
    cursor.execute("DELETE FROM alim_detay WHERE fis_id = ?", (fis_id,))
    for d in detaylar:
        cursor.execute("""
            INSERT INTO alim_detay (fis_id, zeytin_cesidi, kalibre, brut_kilo,
                                    dara, net_kilo, birim_fiyat, tutar)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
            (fis_id, d["zeytin_cesidi"], d["kalibre"], d["brut"], d["dara"],
             d["net"], d["fiyat"], d["net"] * d["fiyat"]))
    conn.commit()
    conn.close()


def get_all_alim_fisleri():
    conn = get_connection()
    rows = conn.execute(
        "SELECT * FROM alim_fisi ORDER BY tarih DESC, id DESC").fetchall()
    conn.close()
    return rows


def get_alim_fisleri_aralik(baslangic, bitis):
    """Tarih araligindaki alim fisleri (baslangic/bitis dahil, YYYY-MM-DD)."""
    conn = get_connection()
    rows = conn.execute(
        "SELECT * FROM alim_fisi WHERE tarih BETWEEN ? AND ? "
        "ORDER BY tarih DESC, id DESC", (baslangic, bitis)).fetchall()
    conn.close()
    return rows


def get_satis_fisleri_aralik(baslangic, bitis):
    """Tarih araligindaki teslimat (satis) fisleri."""
    conn = get_connection()
    rows = conn.execute(
        "SELECT id, firma_adi, tarih, toplam_kg, hesaplanan_tutar, alinan_para, fark "
        "FROM satis_fisi WHERE tarih BETWEEN ? AND ? "
        "ORDER BY tarih DESC, id DESC", (baslangic, bitis)).fetchall()
    conn.close()
    return rows


def get_giderler_aralik(baslangic, bitis):
    """Tarih araligindaki gider kayitlari."""
    conn = get_connection()
    rows = conn.execute(
        "SELECT * FROM giderler WHERE tarih BETWEEN ? AND ? "
        "ORDER BY tarih DESC, id DESC", (baslangic, bitis)).fetchall()
    conn.close()
    return rows


def get_satici_odeme_ozeti_aralik(baslangic, bitis):
    """Tarih araligindaki satici odemelerinin ozeti."""
    conn = get_connection()
    rows = conn.execute("""
        SELECT o.uretici_ad, a.uretici_tel,
               COALESCE(SUM(o.tutar), 0),
               COUNT(o.id),
               MAX(o.tarih)
        FROM odemeler o
        LEFT JOIN alim_fisi a ON a.uretici_ad = o.uretici_ad
        WHERE o.tarih BETWEEN ? AND ?
        GROUP BY o.uretici_ad
        ORDER BY o.uretici_ad COLLATE NOCASE
    """, (baslangic, bitis)).fetchall()
    conn.close()
    return [
        {"ad": r[0], "tel": r[1] or "", "toplam": r[2] or 0.0,
         "adet": r[3] or 0, "son_tarih": r[4] or ""}
        for r in rows
    ]


def get_summary_stats_aralik(baslangic, bitis):
    """Tarih araligina gore ozet istatistikleri (get_summary_stats gibi).

    Satici kalan bakiye araliktaki fis ve odemelerden hesaplanir.
    """
    conn = get_connection()
    row = conn.execute(
        "SELECT SUM(toplam_kg), SUM(hesaplanan_tutar), SUM(odenen_para) "
        "FROM alim_fisi WHERE tarih BETWEEN ? AND ?",
        (baslangic, bitis)).fetchone()
    sum_kilo = row[0] or 0.0
    sum_tutar = row[1] or 0.0
    sum_odenen = row[2] or 0.0

    row_o = conn.execute(
        "SELECT COALESCE(SUM(tutar), 0) FROM odemeler "
        "WHERE tarih BETWEEN ? AND ?", (baslangic, bitis)).fetchone()
    sum_odenen += row_o[0] or 0.0

    row_g = conn.execute(
        "SELECT SUM(tutar) FROM giderler WHERE tarih BETWEEN ? AND ?",
        (baslangic, bitis)).fetchone()
    sum_gider = row_g[0] or 0.0

    row_s = conn.execute(
        "SELECT SUM(toplam_kg), SUM(hesaplanan_tutar) FROM satis_fisi "
        "WHERE tarih BETWEEN ? AND ?", (baslangic, bitis)).fetchone()
    teslim_kilo = row_s[0] or 0.0
    teslim_para = row_s[1] or 0.0
    conn.close()

    return {
        "toplam_kilo": sum_kilo,
        "toplam_alim_tutar": sum_tutar,
        "toplam_odenen": sum_odenen,
        "kalan_bakiye": sum_tutar - sum_odenen,
        "toplam_gider": sum_gider,
        "toplam_maliyet": sum_tutar + sum_gider,
        "birim_maliyet": (sum_tutar + sum_gider) / sum_kilo if sum_kilo > 0 else 0.0,
        "teslim_kilo": teslim_kilo,
        "teslim_para": teslim_para,
        "depo_kalan_kilo": sum_kilo - teslim_kilo,
    }


def get_alim_fisleri_by_uretici(ad):
    conn = get_connection()
    rows = conn.execute(
        "SELECT * FROM alim_fisi WHERE uretici_ad = ? ORDER BY tarih DESC, id DESC",
        (ad,)).fetchall()
    conn.close()
    return rows


def get_alim_detay(fis_id):
    conn = get_connection()
    rows = conn.execute(
        "SELECT * FROM alim_detay WHERE fis_id = ?", (fis_id,)).fetchall()
    conn.close()
    return rows


def delete_alim_fisi(fis_id):
    conn = get_connection()
    conn.execute("DELETE FROM alim_detay WHERE fis_id = ?", (fis_id,))
    conn.execute("DELETE FROM alim_fisi WHERE id = ?", (fis_id,))
    conn.commit()
    conn.close()


def delete_satici(ad):
    conn = get_connection()
    rows = conn.execute("SELECT id FROM alim_fisi WHERE uretici_ad = ?", (ad,)).fetchall()
    for (fis_id,) in rows:
        conn.execute("DELETE FROM alim_detay WHERE fis_id = ?", (fis_id,))
        conn.execute("DELETE FROM alim_fisi WHERE id = ?", (fis_id,))
    # Satıcının sonradan yapılan ödemelerini de sil; yoksa raporda
    # hayalet bakiye kalır (ödeme tutarı fişsiz kalır).
    conn.execute("DELETE FROM odemeler WHERE uretici_ad = ?", (ad,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Satici bakiyeleri
# ---------------------------------------------------------------------------

def get_satici_bakiyeleri(tarih=None):
    conn = get_connection()
    if tarih:
        rows = conn.execute("""
            SELECT uretici_ad, uretici_tel, SUM(toplam_kg), SUM(hesaplanan_tutar),
                   SUM(odenen_para)
            FROM alim_fisi WHERE tarih = ? GROUP BY uretici_ad""", (tarih,)).fetchall()
    else:
        rows = conn.execute("""
            SELECT uretici_ad, uretici_tel, SUM(toplam_kg), SUM(hesaplanan_tutar),
                   SUM(odenen_para)
            FROM alim_fisi GROUP BY uretici_ad""").fetchall()
    # Fis uzerinde odenen + sonradan yapilan odemeler (odemeler tablosu)
    ek_odemeler = {}
    for ad, toplam in conn.execute(
            "SELECT uretici_ad, SUM(tutar) FROM odemeler GROUP BY uretici_ad"):
        ek_odemeler[ad] = toplam or 0.0
    conn.close()
    return [
        {"ad": r[0], "tel": r[1] or "", "toplam_kg": r[2] or 0.0,
         "toplam_tutar": r[3] or 0.0,
         "odenen": (r[4] or 0.0) + ek_odemeler.get(r[0], 0.0),
         "bakiye": (r[3] or 0.0) - (r[4] or 0.0) - ek_odemeler.get(r[0], 0.0)}
        for r in sorted(rows, key=lambda r: r[0].lower())
    ]


def add_odeme(uretici_ad, tarih, tutar, aciklama=""):
    conn = get_connection()
    conn.execute(
        "INSERT INTO odemeler (uretici_ad, tarih, tutar, aciklama) VALUES (?, ?, ?, ?)",
        (uretici_ad, tarih, _tr_to_float(tutar), aciklama))
    conn.commit()
    conn.close()


def get_odemeler_by_uretici(ad):
    conn = get_connection()
    rows = conn.execute(
        "SELECT tarih, tutar, aciklama FROM odemeler WHERE uretici_ad = ? ORDER BY tarih DESC, id DESC",
        (ad,)).fetchall()
    conn.close()
    return rows


def get_satici_odeme_ozeti():
    """Rapor sayfasi icin satici odeme ozeti.

    Her satıcı icin sonradan yapilan odemelerin (odemeler tablosu) toplamini,
    sayisini ve son odeme tarihini dondurur. Fis uzerindeki pesin odemeler
    (odenen_para) ayri olarak alim fisleri tablolarinda gorunur.
    """
    conn = get_connection()
    rows = conn.execute("""
        SELECT o.uretici_ad, a.uretici_tel,
               COALESCE(SUM(o.tutar), 0),
               COUNT(o.id),
               MAX(o.tarih)
        FROM odemeler o
        LEFT JOIN alim_fisi a ON a.uretici_ad = o.uretici_ad
        GROUP BY o.uretici_ad
        ORDER BY o.uretici_ad COLLATE NOCASE
    """).fetchall()
    conn.close()
    return [
        {"ad": r[0], "tel": r[1] or "", "toplam": r[2] or 0.0,
         "adet": r[3] or 0, "son_tarih": r[4] or ""}
        for r in rows
    ]


# ---------------------------------------------------------------------------
# Giderler
# ---------------------------------------------------------------------------

def add_gider(tarih, tur, tutar, aciklama):
    conn = get_connection()
    conn.execute(
        "INSERT INTO giderler (tarih, tur, tutar, aciklama) VALUES (?, ?, ?, ?)",
        (tarih, tur, _tr_to_float(tutar), aciklama))
    conn.commit()
    conn.close()


def get_all_giderler():
    conn = get_connection()
    rows = conn.execute("SELECT * FROM giderler ORDER BY tarih DESC, id DESC").fetchall()
    conn.close()
    return rows


def delete_gider(gider_id):
    conn = get_connection()
    conn.execute("DELETE FROM giderler WHERE id = ?", (gider_id,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Satis (teslimat) fisleri
# ---------------------------------------------------------------------------

def add_satis_fisi(firma_adi, tarih, alinan_para, detaylar):
    toplam_kg = sum(d["kg"] for d in detaylar)
    hesaplanan = sum(d["kg"] * d["fiyat"] for d in detaylar)
    fark = hesaplanan - alinan_para
    conn = get_connection()
    cursor = conn.cursor()
    cursor.execute("""
        INSERT INTO satis_fisi (firma_adi, tarih, toplam_kg, hesaplanan_tutar,
                                alinan_para, fark)
        VALUES (?, ?, ?, ?, ?, ?)""",
        (firma_adi, tarih, toplam_kg, hesaplanan, alinan_para, fark))
    fis_id = cursor.lastrowid
    for d in detaylar:
        cursor.execute("""
            INSERT INTO satis_detay (fis_id, kalibre, kg, birim_fiyat, tutar)
            VALUES (?, ?, ?, ?, ?)""",
            (fis_id, d["kalibre"], d["kg"], d["fiyat"], d["kg"] * d["fiyat"]))
    conn.commit()
    conn.close()
    return fis_id


def get_all_satis_fisleri():
    conn = get_connection()
    rows = conn.execute("""
        SELECT id, firma_adi, tarih, toplam_kg, hesaplanan_tutar, alinan_para, fark
        FROM satis_fisi ORDER BY tarih DESC, id DESC""").fetchall()
    conn.close()
    return rows


def get_satis_detay(fis_id):
    conn = get_connection()
    rows = conn.execute(
        "SELECT kalibre, kg, birim_fiyat, tutar FROM satis_detay WHERE fis_id = ?",
        (fis_id,)).fetchall()
    conn.close()
    return rows


def delete_satis_fisi(fis_id):
    conn = get_connection()
    conn.execute("DELETE FROM satis_detay WHERE fis_id = ?", (fis_id,))
    conn.execute("DELETE FROM satis_fisi WHERE id = ?", (fis_id,))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Teslimat notlari
# ---------------------------------------------------------------------------

def get_teslimat_notlari():
    conn = get_connection()
    row = conn.execute("SELECT kg, para FROM teslimat_notlari WHERE id = 1").fetchone()
    conn.close()
    return {"kg": row[0] if row else "", "para": row[1] if row else ""}


def update_teslimat_notlari(kg, para):
    conn = get_connection()
    conn.execute("UPDATE teslimat_notlari SET kg = ?, para = ? WHERE id = 1",
                 (str(kg or ""), str(para or "")))
    conn.commit()
    conn.close()


# ---------------------------------------------------------------------------
# Ozet istatistikler
# ---------------------------------------------------------------------------

def get_summary_stats():
    conn = get_connection()
    row = conn.execute(
        "SELECT SUM(toplam_kg), SUM(hesaplanan_tutar), SUM(odenen_para) FROM alim_fisi"
    ).fetchone()
    sum_kilo = row[0] or 0.0
    sum_tutar = row[1] or 0.0
    sum_odenen = row[2] or 0.0

    # Sonradan yapilan satici odemeleri de toplam odenene dahil
    row_o = conn.execute("SELECT COALESCE(SUM(tutar), 0) FROM odemeler").fetchone()
    sum_odenen += row_o[0] or 0.0

    row_g = conn.execute("SELECT SUM(tutar) FROM giderler").fetchone()
    sum_gider = row_g[0] or 0.0

    row_s = conn.execute(
        "SELECT SUM(toplam_kg), SUM(hesaplanan_tutar) FROM satis_fisi").fetchone()
    teslim_kilo = row_s[0] or 0.0
    teslim_para = row_s[1] or 0.0
    conn.close()

    return {
        "toplam_kilo": sum_kilo,
        "toplam_alim_tutar": sum_tutar,
        "toplam_odenen": sum_odenen,
        "kalan_bakiye": sum_tutar - sum_odenen,
        "toplam_gider": sum_gider,
        "toplam_maliyet": sum_tutar + sum_gider,
        "birim_maliyet": (sum_tutar + sum_gider) / sum_kilo if sum_kilo > 0 else 0.0,
        "teslim_kilo": teslim_kilo,
        "teslim_para": teslim_para,
        "depo_kalan_kilo": sum_kilo - teslim_kilo,
    }
