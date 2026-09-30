# KARAOĞLU Zeytin Takip — Domain'de Yayınlama Rehberi

Uygulamanız Flask + SQLite tabanlıdır ve **ek ayar gerektirmeden** yayına hazırdır.
Kullanıcı hesapları, şifreler ve tüm veriler `data\` klasöründedir — bu klasörü
**olduğu gibi** sunucuya taşırsanız her şey (üyelikler, onaylar, notlar) aynen devam eder.

---

## YOL A — VPS sunucuda yayınlama (önerilen, kalıcı)

### 1. Sunucu açın
Hetzner / DigitalOcean / AWS Lightsail gibi bir sağlayıcıdan **Ubuntu 24.04** sunucu açın
(en küçük paket yeterli, ~5$/ay). Sunucunun **IP adresini** not edin.

### 2. Dosyaları yükleyin
Bilgisayarınızdan:
```
scp -r "C:\Users\Ateş\Desktop\KARAOGLU_Zeytin_Takip_Web" root@SUNUCU_IP:/opt/karaoglu
```
(Windows'ta WinSCP programı da kullanabilirsiniz.)

### 3. Sunucuda kurun
```
apt update && apt install -y python3-venv python3-pip nginx
cd /opt/karaoglu
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

### 4. Kalıcı servis yapın (sunucu yeniden başlasa da çalışır)
```
cat > /etc/systemd/system/karaoglu.service <<'EOF'
[Unit]
Description=KARAOGLU Zeytin Takip
After=network.target

[Service]
WorkingDirectory=/opt/karaoglu
ExecStart=/opt/karaoglu/.venv/bin/waitress-serve --host=127.0.0.1 --port=8000 wsgi:app
Restart=always

[Install]
WantedBy=multi-user.target
EOF
systemctl enable --now karaoglu
```

### 5. Nginx ile 80 portuna bağlayın
```
cat > /etc/nginx/sites-available/karaoglu <<'EOF'
server {
    listen 80;
    server_name _;
    client_max_body_size 10M;
    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
EOF
ln -sf /etc/nginx/sites-available/karaoglu /etc/nginx/sites-enabled/
rm -f /etc/nginx/sites-enabled/default
nginx -t && systemctl reload nginx
```

### 6. Domain'inizi yönlendirin
Domain panelinize gidin (norumsin, isimtescil, GoDaddy vb.) ve **DNS** bölümünde:
- **A kaydı**: `@` → sunucu IP adresi
- (İsterseniz) **A kaydı**: `www` → sunucu IP adresi

DNS yayılması 5 dakika – 24 saat sürebilir.

### 7. Ücretsiz SSL (https) ekleyin
```
apt install -y certbot python3-certbot-nginx
certbot --nginx -d DOMAININIZ.COM -d www.DOMAININIZ.COM
```
Artık `https://domaininiz.com` adresinden herkes ulaşabilir.

---

## YOL B — Render.com ile yayınlama (kolay, Git gerektirir)

Depoda **render.yaml** (Render Blueprint) hazır; panelde tek tıkla kurulur:

1. Klasörü GitHub'a yükleyin (`.venv`, `data/`, `__pycache__/` hariç — `.gitignore` halleder;
   `data\` klasörünü yüklemek istemezseniz sunucuda ilk kurulumu yeniden yaparsınız).
2. render.com → **New + → Blueprint** → GitHub deponuzu seçin → **Apply**.
   (Blueprint kullanmayacaksanız **Web Service** oluşturun ve şunları girin:
   Build Command: `pip install -r requirements.txt`
   Start Command: `waitress-serve --host=0.0.0.0 --port=$PORT wsgi:app`
   Health Check Path: `/saglik`)
3. Publish sonrası Render size bir adres verir (ör. `karaoglu.onrender.com`).
4. Render → Settings → **Custom Domain** → `domaininiz.com` ekleyin; size verdiği
   **CNAME** değerini domain panelinizde `www` için girin (kök alan için A/ALIAS kaydı).

### Ücretli plan (Starter + Kalıcı Disk) — veri kaybı olmaz

Depodaki **render.yaml** artık ücretli kurulumla hazır: `plan: starter` ($7/ay) +
1 GB **Persistent Disk** ($0.25/ay). Blueprint'i (re)apply ettiğinizde:

- Site artık 15 dk hareketsizlikte **uykuya girmez** (her zaman açık).
- `data/` klasörü `/var/data` altındaki **kalıcı diske** taşınır — Render yeniden
  başlasa da **üyeler, fişler, logo silinmez**.
- Otomatik yedek sistemi (`/yedek-otomatik` + `otomatik_yedek.py`) aynen çalışır.

**İlk geçişte veri kurtarma (önemli):** Ücretsiz plandaki son verileriniz silinmişse,
bilgisayarınızdaki en güncel `C:\KARAOGLU_YEDEK\zeytin_takip_yedek_*.db` dosyasını
alıp **Render Shell**'den (ücretli planda Shell sekmesi açılır):

```bash
cp /tmp/yedek.db /var/data/zeytin_takip.db
# secret_key dosyanızı da aynı klasöre koyun (oturumlar bozulmadan devam eder)
```

veya dosyayı deploy klasörüne koyup `KARAOGLU_DATA_DIR=/var/data` ile yeniden deploy edin.

### Domain bağlama (zeytinhesap.com)

Render → Web Service → **Settings → Custom Domains** → `www.zeytinhesap.com` ekleyin.
Render size bir **CNAME hedefi** verir (orn. `karaoglu-zeytin-takip.onrender.com`).
Domain panelinizde:

| Kayıt tipi | Ad | Değer |
|---|---|---|
| CNAME | `www` | Render'ın verdiği CNAME hedefi |
| URL Redirect | `@` | `https://www.zeytinhesap.com` (kök alanı www'ye yönlendir) |

Render ücretli planda **otomatik SSL (Let's Encrypt)** verir — ekstra işlem gerekmez.
Sonra `otomatik_yedek.py` içinde `SITE_ADRESI = "https://www.zeytinhesap.com"` yapın.

### "Application Loading" ekranında takılıyorsa
- Render ücretsiz planda site 15 dk hareketsizlikte uykuya girer; ilk istek
  **~1 dakika** sürer ve bu sırada "WELCOME TO RENDER" yükleme ekranı görünür.
  **Bir kez bekleyin** — bu normaldir.
- Ekran 2-3 dakikadan uzun takılı kalıyorsa servis açılamıyor demektir:
  Render panelden **Events + Logs** sekmesine bakın. En sık nedenler:
  - Start Command `0.0.0.0` ve `$PORT` kullanmıyor (bizim Procfile/render.yaml doğru komutu içerir).
  - Build sırasında `pip install` hatası (requirements.txt bozuksa).
  - Health check `/saglik` yanıt vermiyor (hata varsa Logs'ta Python traceback görünür).
  - Ücretsiz 750 saat/ay limiti dolduysa servis askıya alınır; planı Starter yapın.
- **Veri kaybı uyarısı:** Ücretsiz planda disk geçicidir; her yeniden başlatmada
  `data/` (veritabanı, kullanıcılar, logo) silinir. Çözüm: yukarıdaki ücretli
  Starter + Persistent Disk kurulumu (render.yaml hazır), otomatik yedek sistemi
  veya YOL A (VPS).

---

## Önemli notlar

- **Veri taşıma:** Yayına almadan önce mevcut `data\` klasörünü (veritabanı + şifre
  anahtarları + logo) sunucudaki klasöre kopyalayın. Kullanıcılar ve üyelikler aynen geçer.
- **Yedek:** Uygulama içindeki "Yedek İndir" düğmesi (yalnızca yönetici) her an
  veritabanını bilgisayarınıza indirir.
- **Güvenlik:** `KARAOGLU_SECRET_KEY` ortam değişkeni verilmezse uygulama `data/secret_key`
  dosyasına kalıcı rastgele anahtar üretir — hiçbir şey yapmanız gerekmez.
- **Güncelleme:** Kod değişikliğinde sadece `app.py`, `webdb.py`, `templates/`, `static/`
  dosyalarını sunucuya kopyalayıp `systemctl restart karaoglu` demeniz yeterlidir.

---

## Otomatik yedekleme (Render silinmelerine karsi)

Render ucretsiz planda her yeniden baslatmada `data/` silindigi icin, bilgisayarinizda
**gunluk otomatik yedek** sistemi hazirdir:

- **`otomatik_yedek.py`** – Veritabanini siteden indirip `C:\KARAOGLU_YEDEK` klasorune kaydeder.
  Son 30 gunun yedegini tutar, eskilerini otomatik siler.
- **`YEDEK_AL.bat`** – Cift tiklayinca hemen yedek alir (elle).
- **`YEDEK_KUR.bat`** – Windows Gorev Zamanlayici'ya ekler: HER GUN 09:00'da otomatik yedek.
- **`YEDEK_SIL.bat`** – Zamanlanmis gorevi kaldirir.

### Ilk kurulum (bir kere)
1. `otomatik_yedek.py` dosyasini not defteriyle acin, `SITE_ADRESI` satirina kendi
   Render adresinizi yazin (orn. `https://karaoglu-zeytin-takip.onrender.com`).
2. Gizli anahtar otomatik bulunur: bilgisayarinizdaki `data/secret_key` dosyasindaki
   deger ile Render'daki `KARAOGLU_SECRET_KEY` AYNI olmalidir. Render Blueprint bu
   degiskeni rastgele uretir; ayni anahtari bilgisayarinizdaki `data/secret_key`
   dosyasina kopyalayin (Render panelden Dashboard > Environment > secret key).
3. `YEDEK_KUR.bat` dosyasina cift tiklayin (yonetici onayi isteyebilir).

### Yedekten geri yukleme (Render silindiginde)
1. `C:\KARAOGLU_YEDEK` icindeki en guncel `zeytin_takip_yedek_*.db` dosyasini alin.
2. Adini `zeytin_takip.db` yapin ve sitenin `data/` klasorune kopyalayin (Render'da
   ucretsiz planda disk silindigi icin yeniden deploy sonrasi `data/` yeniden bosalir;
   dosyayi deploy klasorune koyup yeniden push etmek veya VPS'te dogrudan kopyalamak gerekir).
3. Ayrica `secret_key` dosyanizi da ayni klasore koyun; oturumlar bozulmadan devam eder.

> Not: Yedek yalnizca veritabanidir. Yuklenen logo dosyasi `data/` icinde ayri durur;
> logoyu da korumak isterseniz `C:\KARAOGLU_YEDEK` klasorunu iceren tum `data/`
> yedegini yani sira offline bir kopyasini da saklayin.

---

## Admin kullanici adini / sifresini unuttum (garantili yontem)

Klasorde **ADMIN_OLUSTUR.bat** ve **admin_duzelt.py** dosyalari vardir.

- **Windows'ta:** `ADMIN_OLUSTUR.bat` dosyasina cift tiklayin.
- **Linux sunucuda:** `cd /opt/karaoglu && python3 admin_duzelt.py`

Bu araclar `karaoglu` kullanıcısını yoksa OLUSTURUR, varsa sifresini `1234` olarak
sifirlar; rolunu admin, lisansini aktif yapar. Sonrasinda:

1. Giris ekraninda: kullanici adi `karaoglu`, sifre `1234`
2. Sol menu > Ayarlar > Kullanicilar sayfasindan yeni uyelikleri Onayla/durdur.
3. Guvenlik icin ayni sayfadan admin sifresini kendinize gore degistirin.
