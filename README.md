# KARAOĞLU Zeytin Takip — WEB Sürümü

Masaüstü uygulamasının (app.exe) web tabanlı sürümü. Aynı alan/şema mantığıyla
satıcı alımları, teslimat (satış) fişleri, giderler, kalibre fiyatları ve
komisyon takibi yapar. Flask + SQLite ile yazıldı; ek bağımlılık yok denecek
kadar azdır.

## Klasördeki dosyalar

| Dosya/Klasör        | Açıklama |
|---------------------|----------|
| `app.py`            | Flask web uygulaması (tüm ekranlar) |
| `webdb.py`          | SQLite veri katmanı (`data/zeytin_takip.db`) |
| `webpdf.py`         | Alım fişi PDF üretimi |
| `pdfgen.py`         | Sıfır bağımlılıklı PDF motoru (masaüstüyle aynı) |
| `wsgi.py`           | Yayın sunucusu giriş noktası |
| `templates/`        | Sayfa şablonları |
| `static/`           | CSS, JS, logo |
| `data/`             | Veritabanı burada oluşur (yedeklemeyi unutmayın) |

## 1) Bilgisayarda çalıştırma (deneme)

Python 3.10+ kurulu olmalı. Klasörde:

```bash
python -m venv .venv
.venv\Scripts\activate          # Windows
pip install -r requirements.txt
python app.py
```

Tarayıcıda: http://127.0.0.1:5000

İlk açılışta **İlk Kurulum** ekranından yönetici kullanıcı adı/şifre oluşturulur.
Ardından bu bilgilerle giriş yapılır.

## 2) Kendi alan adınızda (domainde) yayınlama

Veritabanı ve hesap bilgileri barındırdığı için **ücretsiz/ortak** platformlar
yerine kendi sunucunuz (VPS) veya PaaS önerilir. İki yaygın yol:

### A) VPS üzerinde (Ubuntu + Nginx önerilir)

```bash
# Sunucuya kopyalayın (ör. scp -r KARAOGLU_Zeytin_Takip_Web/ kullanici@sunucu:~/karaoglu-web)
cd ~/karaoglu-web
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
# Üretim sunucusu (waitress):
waitress-serve --host=127.0.0.1 --port=8000 wsgi:app
```

Nginx site yapılandırması (`/etc/nginx/sites-available/karaoglu`):

```nginx
server {
    listen 80;
    server_name alanadiniz.com;          # ve/veya www.alanadiniz.com

    location /static/ {
        alias /home/kullanici/karaoglu-web/static/;
        expires 7d;
    }
    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

```bash
sudo ln -s /etc/nginx/sites-available/karaoglu /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

Alan adınızın DNS **A** kaydını sunucu IP'nize yönlendirin.
HTTPS için (önerilir):

```bash
sudo apt install certbot python3-certbot-nginx
sudo certbot --nginx -d alanadiniz.com
```

Sunucu yeniden başlayınca otomatik çalışması için systemd servisi
(`/etc/systemd/system/karaoglu-web.service`):

```ini
[Unit]
Description=KARAOĞLU Zeytin Takip Web
After=network.target

[Service]
User=kullanici
WorkingDirectory=/home/kullanici/karaoglu-web
Environment=KARAOGLU_SECRET_KEY=cok-gizli-uzun-bir-anahtar
ExecStart=/home/kullanici/karaoglu-web/.venv/bin/waitress-serve --host=127.0.0.1 --port=8000 wsgi:app
Restart=always

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now karaoglu-web
```

### B) Render.com ile yayınlama

Depoda `render.yaml` (Render Blueprint) hazır: panelde tek tıkla tüm ayarlar
otomatik gelir.

1. Kodu GitHub'a yükleyin (`data/`, `.venv/`, `__pycache__/` hariç — `.gitignore` halleder).
2. render.com → **New + → Blueprint** → deponuzu seçin → **Apply**.
   (Blueprint yerine **Web Service** kurarsanız:
   Build: `pip install -r requirements.txt`,
   Start: `waitress-serve --host=0.0.0.0 --port=$PORT wsgi:app`,
   Health Check Path: `/saglik`)
3. Deploy bitince Render size `https://<servis-adı>.onrender.com` adresi verir.
4. Custom Domain ile alan adınızı bağlayın.

**Önemli — ücretsiz plan sınırları:**

- Site 15 dakika trafik almazsa **uykuya girer**; sonraki ilk istek ~1 dakika
  sürer ve bu sırada Render'ın "WELCOME TO RENDER / APPLICATION LOADING"
  sayfası görünür. Bu ekran 1-2 dakikadan uzun takılı kalıyorsa servis
  açılamamıştır: Render panelden **Logs** sekmesine bakın (aşağıya bakın).
- Dosya sistemi geçicidir: her yeniden başlatmada `data/` içeriği (SQLite
  veritabanı, logo, kullanıcılar) **silinir**. Verilerinizin kalıcı olması için
  ücretli planda **Persistent Disk** ekleyin (mount: `/opt/karaoglu/data`,
  `KARAOGLU_DATA_DIR=/opt/karaoglu/data`) veya VPS (A seçeneği) kullanın.

**"Application Loading" ekranında takılıyorsa (kontrol listesi):**

1. Render → Logs: `Now listening on port 10000` benzeri satır var mı? Yoksa
   start komutu hatalıdır (Procfile/render.yaml'daki komut kullanılır).
2. Start komutu `0.0.0.0` ve `$PORT` kullanmalı; `127.0.0.1` veya sabit port
   Render'da çalışmaz.
3. Health Check Path `(/saglik)` 200 dönmeli; panelden "Check /saglik" ile test edin.
4. Build loglarında `pip install -r requirements.txt` hatası varsa bağımlılıkları
   düzeltin (`flask>=3.0`, `waitress>=3.0`).
5. Ücretsiz plan instance saat limitini (750 saat/ay) doldurduysa servis ay sonuna
   kadar askıya alınır; Settings → planı `Starter` yapın.

## Güvenlik notları

- `KARAOGLU_SECRET_KEY` ortam değişkenini mutlaka uzun/rastgele belirleyin.
- HTTPS kullanın (Let's Encrypt ücretsizdir).
- `data/` klasörünü düzenli yedekleyin (Ayarlar → Yedek İndir de mevcut).
- Şifreler PBKDF2-SHA256 ile hash'lenir; hiçbir yerde düz metin saklanmaz.

## Masaüstü verilerinizi taşımak isterseniz

Masaüstü uygulamasının veritabanı şeması birebir aynıdır:
`%APPDATA%\ZeytinTakip\zeytin_takip.db` dosyasını bu klasördeki `data/`
içine `zeytin_takip.db` adıyla kopyalamanız yeterlidir; web sürümü aynen
kullanır.
