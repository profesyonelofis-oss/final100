@echo off
chcp 65001 >nul
title KARAOGLU Zeytin Takip - Otomatik Yedek
echo ============================================================
echo  KARAOGLU ZEYTIN TAKIP - OTOMATIK YEDEK ALICI
echo ============================================================
echo.
echo  Bu betik Render sitesinden veritabanini indirip
echo  C:\KARAOGLU_YEDEK klasorune kaydeder.
echo.
echo  --- ONEMLI: Ilk kullanimda ayari yapin ---
echo  otomatik_yedek.py dosyasini not defteri ile acip
echo  SITE_ADRESI satirina kendi Render adresinizi yazin:
echo    orn: https://karaoglu-zeytin-takip.onrender.com
echo.
echo  Gorev Zamanlayici'ya eklemek icin: YEDEK_KUR.bat
echo ============================================================
echo.
cd /d "%~dp0"
python otomatik_yedek.py
echo.
pause
