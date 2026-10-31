# KARAOĞLU Zeytin Takip — GitHub + Render Yükleme Paketi

Bu klasör, `profesyonelofis-oss/final100` deposuna yüklenecek **tam pakettir**.
Tüm dosyalar deponun KÖKÜNE gider (alt klasör yok) — Render tam olarak bunu bekler.

## 1) GitHub'a yükleme

**Tek tık:** Bu klasördeki **YUKLE_GITHUB.bat** dosyasına çift tıklayın →
tarayıcıdan GitHub girişi yapın → `TAMAM` yazısını bekleyin.

Bitince şurada kontrol edin: https://github.com/profesyonelofis-oss/final100
`requirements.txt`, `app.py`, `render.yaml`, `Procfile`... dosyalarını
**anasayfada (kökte)** görmelisiniz. Klasör görüyorsanız yanlış yüklenmiştir.

## 2) Render'a bağlama

1. https://render.com → giriş yapın.
2. Önceki hatalı servis varsa: servisi açın → **Settings** → en altta
   **Delete Service** ile silin (temiz başlangıç).
3. Panoda **New +** → **Blueprint** → `final100` deposunu seçin → **Connect**.
4. Render `render.yaml` dosyasını okur, her şeyi kendisi ayarlar → **Apply**.
5. 2-3 dakikada yayınlanır. Adres: `https://karaoglu-zeytin-takip.onrender.com`

Elle kurmak isterseniz (Blueprint yerine Web Service):
- Runtime: **Python 3**
- Build Command: `pip install -r requirements.txt`
- Start Command: `waitress-serve --host=0.0.0.0 --port=$PORT wsgi:app`
- Health Check Path: `/saglik`

## 3) İlk kurulum

Site açılınca **İLK KURULUM** ekranı gelir: yönetici kullanıcı adı/şifre
oluşturup giriş yaparsınız. Eski verilerinizi geri yüklemek isterseniz
(Toplu klasöründeki `KARAOGLU_WEB_YAYIN_db_yedek_20260926_2116.db` dosyası):
Render'da ücretsiz plan diski geçici olduğu için veri kalıcılığı YOKTUR;
kalıcı kullanım için ücretli plan + Persistent Disk veya VPS gerekir.

## Notlar

- GitHub'a her push'ta Render otomatik yeniden yayınlar.
- Ücretsiz planda site 15 dk trafik almazsa uykuya girer; ilk istek ~1 dk sürer
  (Render yükleme ekranı görünür — normaldir, bekleyin).
- Deploy hata verirse Render → Logs sekmesinin görüntüsünü alın.
