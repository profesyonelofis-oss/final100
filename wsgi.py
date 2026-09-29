# -*- coding: utf-8 -*-
"""WSGI giris noktasi (gunicorn / waitress / IIS icin).

Kullanim:
    waitress-serve --host=0.0.0.0 --port=8000 wsgi:app
veya
    gunicorn -b 0.0.0.0:8000 wsgi:app
"""
import os

from app import app
import webdb

webdb.init_db()

if __name__ == "__main__":
    port = int(os.environ.get("PORT", 8000))
    from waitress import serve
    serve(app, host="0.0.0.0", port=port)
