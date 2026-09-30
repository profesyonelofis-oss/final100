@echo off
chcp 65001 >nul
title KARAOGLU - Yedek Gorevini Sil
echo Gunluk otomatik yedek gorevi siliniyor...
schtasks /Delete /F /TN "KARAOGLU Gunluk Yedek"
echo.
echo Islem tamam. Manuel yedek icin YEDEK_AL.bat kullanabilirsiniz.
pause
