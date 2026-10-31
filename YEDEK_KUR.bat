@echo off
chcp 65001 >nul
title KARAOGLU - Gunluk Yedek Gorevini Kur
echo ============================================================
echo  GUNLUK OTOMATIK YEDEK KURULUMU
echo ============================================================
echo.
echo  Windows her gun saat 09:00'da otomatik yedek alacak.
echo  (Bilgisayar o saatte kapaliysa acilinca alir.)
echo.
echo  once otomatik_yedek.py icindeki SITE_ADRESI'ni kontrol edin!
echo.
pause
cd /d "%~dp0"

set PYTHON_CMD=python
where python >nul 2>nul || set PYTHON_CMD=py

%PYTHON_CMD% -c "import sys; print(sys.executable)" > "%TEMP%\karaoglu_python_yolu.txt" 2>nul
set /p PY_YOL=<"%TEMP%\karaoglu_python_yolu.txt"

schtasks /Create /F /SC DAILY /ST 09:00 /TN "KARAOGLU Gunluk Yedek" ^
  /TR "\"%PY_YOL%\" \"%~dp0otomatik_yedek.py\""

if %errorlevel%==0 (
  echo.
  echo  OK! Gorev kuruldu: "KARAOGLU Gunluk Yedek" - her gun 09:00
  echo  Yedekler C:\KARAOGLU_YEDEK klasorune kaydedilir.
  echo  Silmek isterseniz YEDEK_SIL.bat dosyasina cift tiklayin.
) else (
  echo.
  echo  HATA: Gorev kurulamadi. Programi "Yonetici olarak calistirin".
)
echo.
pause
