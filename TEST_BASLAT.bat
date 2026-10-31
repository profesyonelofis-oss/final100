@echo off
title KARAOGLU - Yerel Test Sunucusu
cd /d "%~dp0"

echo ============================================================
echo  YEREL TEST SUNUCUSU baslatiliyor...
echo  Tarayicida acin:  http://127.0.0.1:5000
echo  Durdurmak icin bu pencereyi kapatin.
echo  (Veriler bu klasordeki data\ klasorune yazilir)
echo ============================================================
echo.

.venv\Scripts\python.exe app.py
pause
