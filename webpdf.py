# -*- coding: utf-8 -*-
"""KARAOGLU Zeytin Takip - fis PDF uretimi (pdfgen uzerinden).

Not: Turkce karakterler kaynakta unicode kacisleriyle yazilmistir ki dosya
her ortamda birebir ayni icerigi tasisin.
"""

from datetime import datetime

import pdfgen
import webdb


def _fmt(x):
    return webdb.fmt_tr(x, True)


def _fmt_kg(x):
    return webdb.fmt_tr(x, False)


def alim_fisi_pdf(fis, detay):
    """fis: alim_fisi satiri (tuple), detay: alim_detay satirlari."""
    (fis_id, uretici_ad, uretici_tel, tarih, toplam_kg, hesaplanan,
     odenen, fark) = fis[0], fis[1], fis[2], fis[3], fis[4], fis[5], fis[6], fis[7]

    L = {
        "baslik": webdb.get_firma_adi(),
        "fis": "ALIM F\u0130\u015e\u0130",
        "satici": "SATICI: ",
        "tel": "TELEFON: ",
        "tarih": "TAR\u0130H: ",
        "cesit": "ZEYT\u0130N \u00c7E\u015e\u0130D\u0130",
        "kalibre": "KAL\u0130BRE",
        "kg": "KG",
        "fiyat": "F\u0130YAT (\u20ba/KG)",
        "tutar": "TUTAR (\u20ba)",
        "hesaplanan": "Hesaplanan Tutar",
        "odenen": "\u00d6denen",
        "kalan": "KALAN BOR\u00c7",
        "imza": ("\u0130mza (Sat\u0131c\u0131): ________________     "
                 "\u0130mza (Al\u0131c\u0131): ________________"),
    }

    doc = pdfgen.PDFDoc(pdfgen.A4)
    doc.flow_begin(margin=40, bottom=50)
    doc.flow_set_title("Al\u0131m Fi\u015fi #%s - %s" % (fis_id, tarih))

    # Baslik
    doc.flow_text(doc.W - doc._fm, doc.top(doc._fm) + 6,
                  L["baslik"], style="b", size=18, align="right")
    doc.flow_text(doc.W - doc._fm, doc.top(doc._fm) - 12,
                  datetime.now().strftime("%d.%m.%Y %H:%M"),
                  size=9, align="right")
    doc._cur -= 14
    doc.flow_text(doc._fm, doc._cur, L["fis"], style="b", size=16)
    doc._cur -= 8
    doc.hline(doc._fm, doc._cur, doc.W - doc._fm, 1.2)
    doc._cur -= 20

    # Satici bilgileri
    doc.flow_text(doc._fm, doc._cur, L["satici"] + uretici_ad, style="b", size=13)
    doc.flow_text(doc.W / 2, doc._cur, L["tel"] + (uretici_tel or "-"), size=11)
    doc.flow_text(doc.W - doc._fm, doc._cur, L["tarih"] + tarih, size=11,
                  align="right")
    doc._cur -= 24

    # Tablo
    doc.table_header([
        (L["cesit"], 140, None),
        (L["kalibre"], 80, None),
        (L["kg"], 70, "right"),
        (L["fiyat"], 90, "right"),
        (L["tutar"], 95, "right"),
    ])
    for d in detay:
        # d: id, fis_id, cesit, kalibre, brut, dara, net, fiyat, tutar
        doc.table_row([d[2], d[3], _fmt_kg(d[6]), _fmt(d[7]), _fmt(d[8])])
        doc.table_sep()
    doc.table_end("%s kalem / %s KG / %s \u20ba" %
                  (len(detay), _fmt_kg(toplam_kg), _fmt(hesaplanan)))

    # Odeme ozeti
    doc.flow_ensure(110)
    doc._cur -= 10
    x_lbl, x_val = doc._fm, doc.W - doc._fm
    satirlar = [
        (L["hesaplanan"], hesaplanan, "n"),
        (L["odenen"], odenen, "n"),
        (L["kalan"], fark, "b"),
    ]
    for etiket, deger, stil in satirlar:
        doc.flow_text(x_lbl, doc._cur, etiket, style=stil, size=11)
        doc.flow_text(x_val, doc._cur, "%s \u20ba" % _fmt(deger), style=stil,
                      size=11, align="right")
        doc._cur -= doc.lh(11)
        if stil == "b":
            doc.hline(x_val - 150, doc._cur + 6, x_val)

    doc.flow_text(doc._fm, doc._fb + 26, L["imza"], size=9)

    return doc.build("Al\u0131m Fi\u015fi #%s" % fis_id)
