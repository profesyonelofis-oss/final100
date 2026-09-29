@echo off
title KARAOGLU - final100 Yukleyici
cd /d "%~dp0"

echo ============================================================
echo  Bu klasordeki dosyalar su depoya yuklenecek:
echo  https://github.com/profesyonelofis-oss/final100
echo.
echo  Tum dosyalar repo KOKUNE yuklenir (alt klasor YOK).
echo  Tarayici penceresi acilirsa GitHub girisi yapin.
echo ============================================================
echo.
pause

git init -b main
git add -A
git commit -m "KARAOGLU Zeytin Takip - Render deploy paketi"
git branch -M main
git remote remove origin
git remote add origin https://github.com/profesyonelofis-oss/final100.git
git push -f origin main

echo.
echo ============================================================
echo  Islem bitti. Ustte "Repository not found" veya
echo  "rejected" yaziyorsa hata olmustur; ekran goruntusunu alin.
echo  Sorun yoksa: Render panosunda yeni deploy otomatik baslar.
echo  (Manuel baslatmak icin: Manual Deploy - Deploy latest commit)
echo ============================================================
pause
