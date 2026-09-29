"""Minimal PDF uretici: gercek TTF subset + metin/cizgi sayfalari.

Tasarim:
  - Harici bagimlilik yok (stdlib only): struct, zlib, os.
  - Sistem TTF (arial.ttf + arialbd.ttf) parse edilir; kullanilan glifler
    subset edilip PDF'e gomulur (FontFile2). Turkce karakterler tam destekli.
  - PDF tarafi: Type0 font + Identity-H + CIDFontType2 (CID = subset glyph
    index) + ToUnicode CMap. Metin icerigi 2 baytlik CID hex dizileriyle
    yazilir; kalin metin gercek Bold fontla (F2) basilir.
  - Glif bulunamazsa fontun .notdef glifi kullanilir.
  - TTF yuklenemezse Type1 (Helvetica) fallback: latin1 disi karakterler
    '?' olur (acil durum yolu).

Kullanim:
    doc = pdfgen.PDFDoc(pdfgen.A4)
    doc.text(50, doc.top(50), "SATICI: Ali", style="b", size=14)
    doc.text(doc.W - 50, doc.top(50), "12.345,67 TL", style="b", size=14,
             align="right")
    doc.hline(50, doc.top(70), doc.W - 50)
    data = doc.build("Zeytin Takip - Alim Fisi")
    open("fis.pdf", "wb").write(data)

Cok sayfali akis (sayfa numarasi + tablo basligi tekrari):
    doc = pdfgen.PDFDoc(pdfgen.A4)
    doc.flow_begin(margin=40, bottom=46)      # footer: 'Sayfa X / Y'
    doc.flow_set_title("Satici Raporu")       # footer'in sol metni
    doc.table_header([("URUN", 200, None), ("KG", 80, "right"),
                      ("TUTAR", 219, "right")])
    for satir in veriler:
        doc.table_row(satir)
        doc.table_sep()                       # opsiyonel ince cizgi
    doc.table_end("12 adet / 3.456,78 TL")    # alt cizgi + TOPLAM satiri
    data = doc.build("Satici Raporu")
"""

import os
import struct
import zlib

A4 = (595.28, 841.89)

_LOCAL_FONTS = str(__import__("pathlib").Path(__file__).resolve().parent / "fonts")

_REGULAR_CANDIDATES = (
    _LOCAL_FONTS + "/ZeytinSans.ttf",
    _LOCAL_FONTS + "/font.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/Arial.ttf",
    "C:/Windows/Fonts/calibri.ttf",
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/tahoma.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
)
_BOLD_CANDIDATES = (
    _LOCAL_FONTS + "/ZeytinSans-Bold.ttf",
    _LOCAL_FONTS + "/font-bold.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
    "C:/Windows/Fonts/Arialbd.ttf",
    "C:/Windows/Fonts/calibrib.ttf",
    "C:/Windows/Fonts/segoeuib.ttf",
    "C:/Windows/Fonts/tahomabd.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
)


def find_font(candidates):
    for p in candidates:
        try:
            if os.path.isfile(p):
                return p
        except OSError:
            pass
    return None


# ------------------------------------------------------------------ TTF parse
class _FontError(Exception):
    pass


class _Font:
    """Isimize yarar kadar TTF parser: head/maxp/cmap4/hhea/hmtx/loca/glyf."""

    def __init__(self, path):
        try:
            with open(path, "rb") as fh:
                self.data = fh.read()
        except OSError as exc:
            raise _FontError(str(exc))
        if len(self.data) < 12:
            raise _FontError("dosya cok kisa")
        self.tables = {}
        self._parse_dir()
        self._parse_head()
        self._parse_maxp()
        self._parse_cmap()
        self._parse_hhea()
        self._parse_hmtx()
        self._parse_loca()

    def u16(self, off):
        return struct.unpack(">H", self.data[off:off + 2])[0]

    def s16(self, off):
        return struct.unpack(">h", self.data[off:off + 2])[0]

    def u32(self, off):
        return struct.unpack(">I", self.data[off:off + 4])[0]

    def _parse_dir(self):
        num = self.u16(4)
        p = 12
        for _ in range(num):
            tag = self.data[p:p + 4]
            if len(tag) < 4:
                break
            self.tables[tag.decode("latin1")] = self.u32(p + 8)
            p += 16

    def _parse_head(self):
        off = self.tables.get("head")
        if off is None:
            raise _FontError("head yok")
        if self.u32(off + 12) != 0x5F0F3CF5:
            raise _FontError("head magic uyumsuz")
        self.unitsPerEm = self.u16(off + 18)
        self.flags = self.u16(off + 16)
        self.xMin = self.s16(off + 36)
        self.yMin = self.s16(off + 38)
        self.xMax = self.s16(off + 40)
        self.yMax = self.s16(off + 42)
        self.macStyle = self.u16(off + 44)
        self.indexToLocFormat = self.s16(off + 50)

    def _parse_maxp(self):
        off = self.tables.get("maxp")
        if off is None:
            raise _FontError("maxp yok")
        self.numGlyphs = self.u16(off + 4)
        self.maxp_body = self.data[off + 6:off + 32]

    def _parse_cmap(self):
        off = self.tables.get("cmap")
        if off is None:
            raise _FontError("cmap yok")
        n = self.u16(off + 2)
        best = None
        for i in range(n):
            pid = self.u16(off + 4 + i * 8)
            eid = self.u16(off + 6 + i * 8)
            so = self.u32(off + 8 + i * 8)
            if (pid, eid) == (3, 1):
                best = off + so
                break
            if (pid, eid) in ((0, 3), (0, 4), (3, 10)) and best is None:
                best = off + so
        if best is None or self.u16(best) != 4:
            raise _FontError("cmap format 4 bulunamadi")
        segX2 = self.u16(best + 6)
        seg = segX2 // 2
        endo = best + 14
        sto = endo + segX2 + 2
        do = sto + segX2
        ro = do + segX2
        self._cm = {
            "ends": [self.u16(endo + 2 * i) for i in range(seg)],
            "starts": [self.u16(sto + 2 * i) for i in range(seg)],
            "deltas": [self.s16(do + 2 * i) for i in range(seg)],
            "ros": [self.u16(ro + 2 * i) for i in range(seg)],
            "ro_base": ro + segX2,
        }

    def glyph_id(self, ch):
        cp = ord(ch)
        cm = self._cm
        for i in range(len(cm["ends"])):
            if cm["starts"][i] <= cp <= cm["ends"][i]:
                ro = cm["ros"][i]
                if ro == 0:
                    return (cp + cm["deltas"][i]) & 0xFFFF
                addr = cm["ro_base"] + ro + 2 * (cp - cm["starts"][i])
                if addr + 2 > len(self.data):
                    return 0
                gid = self.u16(addr)
                if gid:
                    gid = (gid + cm["deltas"][i]) & 0xFFFF
                return gid
        return 0

    def _parse_hhea(self):
        off = self.tables.get("hhea")
        if off is None:
            raise _FontError("hhea yok")
        self.ascent = self.s16(off + 4)
        self.descent = self.s16(off + 6)
        self.lineGap = self.s16(off + 8)
        self.advMax = self.u16(off + 10)
        self.minLSB = self.s16(off + 12)
        self.minRSB = self.s16(off + 14)
        self.xMaxExtent = self.s16(off + 16)
        self.numHMetrics = self.u16(off + 34)

    def _parse_hmtx(self):
        off = self.tables.get("hmtx")
        if off is None:
            raise _FontError("hmtx yok")
        n = min(self.numHMetrics, self.numGlyphs)
        advs = [self.u16(off + 4 * i) for i in range(n)]
        if self.numGlyphs > n and advs:
            advs.extend([advs[-1]] * (self.numGlyphs - n))
        self.advs = advs

    def _parse_loca(self):
        off = self.tables.get("loca")
        if off is None or "glyf" not in self.tables:
            raise _FontError("loca/glyf yok")
        self.glyfOff = self.tables["glyf"]
        if self.indexToLocFormat == 0:
            self.locs = [2 * self.u16(off + 2 * i)
                         for i in range(self.numGlyphs + 1)]
        else:
            self.locs = [self.u32(off + 4 * i)
                         for i in range(self.numGlyphs + 1)]

    def glyph_bytes(self, gid):
        return self.data[self.glyfOff + self.locs[gid]:
                         self.glyfOff + self.locs[gid + 1]]

    def glyph_advance(self, gid):
        return self.advs[gid] if gid < len(self.advs) else 0


# ---------------------------------------------------------- subset programi
def _composite_components(gb):
    """Bilesik glifin bilesen gid listesi (degerlendirme sirasinda)."""
    out = []
    p = 10
    while p + 4 <= len(gb):
        flags, gid2 = struct.unpack(">HH", gb[p:p + 4])
        p += 4
        out.append(gid2)
        if flags & 0x0001:
            p += 4
        else:
            p += 2
        if flags & 0x0008:
            p += 2
        elif flags & 0x0040:
            p += 4
        elif flags & 0x0080:
            p += 8
        if not (flags & 0x0020):
            break
    return out


def _table_checksum(data):
    data = data + b"\x00" * (-len(data) % 4)
    total = 0
    for i in range(0, len(data), 4):
        total = (total + struct.unpack(">I", data[i:i + 4])[0]) & 0xFFFFFFFF
    return total


def _remap_composite(gb, gid_pos):
    """Bilesik glifin bilesen gid referanslarini subset indekslerine cevirir."""
    out = bytearray(gb[:10])
    p = 10
    while True:
        if p + 4 > len(gb):
            return bytes(gb)
        flags, gid2 = struct.unpack(">HH", gb[p:p + 4])
        p += 4
        out += struct.pack(">HH", flags, gid_pos.get(gid2, 0))
        if flags & 0x0001:            # ARG_1_AND_2_ARE_WORDS
            p += 4
        else:
            p += 2
        if flags & 0x0008:            # WE_HAVE_A_SCALE
            p += 2
        elif flags & 0x0040:          # X_AND_Y_SCALE
            p += 4
        elif flags & 0x0080:          # TWO_BY_TWO
            p += 8
        if not (flags & 0x0020):      # MORE_COMPONENTS
            break
    return bytes(out)                  # WE_HAVE_INSTRUCTIONS atlanir (gecerli)


def _build_subset_program(font, gids, gid_pos):
    """Sadece gids listesindeki glifleri iceren gecerli bir TTF programi."""
    parts = []
    loca_offsets = []
    cur = 0
    for gid in gids:
        loca_offsets.append(cur)
        if gid < len(font.locs) - 1 and font.locs[gid] < font.locs[gid + 1]:
            gb = font.glyph_bytes(gid)
            if len(gb) >= 10 and struct.unpack(">h", gb[0:2])[0] < 0:
                gb = _remap_composite(gb, gid_pos)
            if len(gb) % 2:
                gb += b"\x00"          # short loca icin cift uzunluk
            parts.append(gb)
            cur += len(gb)
    glyf = b"".join(parts)
    loca_offsets.append(cur)
    loca = b"".join(struct.pack(">H", o // 2) for o in loca_offsets)

    n = len(gids)
    tables = []
    # head (54 bayt, indexToLocFormat=0)
    head = struct.pack(
        ">IIIIHHQQhhhhHHhhh",
        0x00010000, 0x00010000, 0, 0x5F0F3CF5,
        font.flags, font.unitsPerEm, 0, 0,
        font.xMin, font.yMin, font.xMax, font.yMax,
        font.macStyle & 0x3, 8, 0, 0, 0)
    tables.append((b"head", head))
    # hhea (36 bayt)
    hhea = struct.pack(
        ">IhhhhhhhhhhhhhhhH",
        0x00010000, font.ascent, font.descent, font.lineGap,
        font.advMax, font.minLSB, font.minRSB, font.xMaxExtent,
        1, 0, 0, 0, 0, 0, 0, 0, n)
    tables.append((b"hhea", hhea))
    # hmtx: her glifin kendi advance'i, lsb = 0
    hmtx = b"".join(struct.pack(">Hh", font.glyph_advance(g), 0) for g in gids)
    tables.append((b"hmtx", hmtx))
    # maxp v1.0 (32 bayt; kalan alanlar orijinalden — ust sinir degerleri, guvenli)
    tables.append((b"maxp", struct.pack(">IH", 0x00010000, n) + font.maxp_body))
    # cmap: format 4, tek segment 0..FFFF, kimlik esleme (cp -> cp)
    sub = struct.pack(">HHHHHHH", 4, 24, 0, 2, 2, 0, 0)
    sub += struct.pack(">H", 0xFFFF)               # endCode[0]
    sub += struct.pack(">H", 0x0000)               # reservedPad
    sub += struct.pack(">HH", 0x0000, 0x0000)      # startCode[0] + idDelta
    sub += struct.pack(">H", 0x0000)               # idRangeOffset[0]
    cmap = struct.pack(">HHHHI", 0, 1, 3, 1, 12) + sub
    tables.append((b"cmap", cmap))
    tables.append((b"loca", loca))
    tables.append((b"glyf", glyf))
    # name: 1 kayit (family "ZeytinSub"), post v3.0 (32 bayt)
    fam = "ZeytinSub".encode("utf-16-be")
    name = struct.pack(">HHH", 0, 1, 18)
    name += struct.pack(">HHHHHH", 3, 1, 0x409, 1, len(fam), 0) + fam
    tables.append((b"name", name))
    tables.append((b"post", struct.pack(">IihhIIIII", 0x00030000, 0, 0, 0,
                                        0, 0, 0, 0, 0)))

    ntab = len(tables)
    sr = 1
    while sr * 2 <= ntab:
        sr *= 2
    search_range = sr * 16
    entry_sel = sr.bit_length() - 1
    range_shift = ntab * 16 - search_range
    offset = 12 + 16 * ntab
    dirent = b""
    body = b""
    for tag, data in tables:
        padded = data + b"\x00" * (-len(data) % 4)
        dirent += tag + struct.pack(">III", _table_checksum(data),
                                    offset, len(data))
        body += padded
        offset += len(padded)
    prog = (struct.pack(">IHHHH", 0x00010000, ntab, search_range,
                        entry_sel, range_shift) + dirent + body)
    # head.checkSumAdjustment
    adj = (0xB1B0AFBA - _table_checksum(prog)) & 0xFFFFFFFF
    prog = prog[:8] + struct.pack(">I", adj) + prog[12:]
    return prog


# --------------------------------------------------------------- PDF yazar
class _Writer:
    def __init__(self):
        self.objs = []

    def add(self, body):
        self.objs.append(body)
        return len(self.objs)

    def build(self, root, info):
        buf = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        offsets = {}
        for i, body in enumerate(self.objs):
            num = i + 1
            offsets[num] = len(buf)
            buf += b"%d 0 obj\n" % num
            buf += body
            buf += b"\nendobj\n"
        xref_off = len(buf)
        mx = len(self.objs)
        buf += b"xref\n0 %d\n" % (mx + 1)
        buf += b"0000000000 65535 f \n"
        for n in range(1, mx + 1):
            buf += b"%010d 00000 n \n" % offsets[n]
        buf += (b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\n"
                b"startxref\n%d\n%%%%EOF\n" % (mx + 1, root, info, xref_off))
        return bytes(buf)


class _Subsetter:
    """Karakter -> subset glyph indeksi (CID); kullanilan glifleri toplar."""

    def __init__(self, font):
        self.font = font
        self.gids = [0]
        self.gid_pos = {0: 0}
        self.cid_char = {0: "?"}

    def cid(self, ch):
        return self._add(self.font.glyph_id(ch), ch)

    def _add(self, gid, ch="?"):
        pos = self.gid_pos.get(gid)
        if pos is not None:
            # Bilesik glif bileşeni olarak gelmis olabilir; gercek karakter
            # sonradan kodlanirsa '?' yer tutucusunu guncelle.
            if ch != "?" and self.cid_char.get(pos) == "?":
                self.cid_char[pos] = ch
            return pos
        pos = len(self.gids)
        self.gids.append(gid)
        self.gid_pos[gid] = pos
        self.cid_char[pos] = ch
        # Bilesik glifse bilesenleri de alt kumeye ekle (ozyinelemeli):
        f = self.font
        if gid < len(f.locs) - 1 and f.locs[gid] < f.locs[gid + 1]:
            gb = f.glyph_bytes(gid)
            if len(gb) >= 10 and struct.unpack(">h", gb[0:2])[0] < 0:
                for comp in _composite_components(gb):
                    self._add(comp)
        return pos

    def encode(self, text):
        return b"".join(b"%04X" % self.cid(ch) for ch in text)

    def width_pt(self, text, size):
        upm = self.font.unitsPerEm
        return sum(self.font.glyph_advance(self.font.glyph_id(ch))
                   for ch in text) * size / upm


def _utf16_hex(s):
    return b"<" + s.encode("utf-16-be").hex().upper().encode("latin1") + b">"


def _tounicode_stream(cid_char):
    out = ["/CIDInit /ProcSet findresource begin",
           "12 dict begin",
           "begincmap",
           "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) "
           "/Supplement 0 >> def",
           "/CMapName /Adobe-Identity-UCS def",
           "/CMapType 2 def",
           "1 begincodespacerange",
           "<0000> <FFFF>",
           "endcodespacerange"]
    entries = []
    for cid in sorted(cid_char):
        ch = cid_char[cid]
        cps = [ord(ch)] if ord(ch) < 0x10000 else \
            [0xD800 + ((ord(ch) - 0x10000) >> 10),
             0xDC00 + ((ord(ch) - 0x10000) & 0x3FF)]
        hexcp = "".join("%04X" % c for c in cps)
        entries.append("<%04X> <%s>" % (cid, hexcp))
    for i in range(0, len(entries), 100):
        chunk = entries[i:i + 100]
        out.append("%d beginbfchar" % len(chunk))
        out.extend(chunk)
        out.append("endbfchar")
    out += ["endcmap",
            "CMapName currentdict /CMap defineresource pop",
            "end", "end"]
    return ("\n".join(out)).encode("latin1")


class PDFDoc:
    """Tek sayfalik PDF belgesi. text()/hline()/rect() ile cizilir, build()
    ile bayt cikti uretilir. text() koordinatlari: x soldan, y ALT kenardan
    (baseline). doc.top(margin) ilk satir baseline'i icin pratik yardimci."""

    def __init__(self, pagesize=A4):
        self.W, self.H = float(pagesize[0]), float(pagesize[1])
        self._ops = []
        self._flow = False
        self._pages = []
        self._ttf = True
        try:
            reg_p = find_font(_REGULAR_CANDIDATES)
            bold_p = find_font(_BOLD_CANDIDATES) or reg_p
            if not reg_p:
                raise _FontError("sistem fontu bulunamadi")
            self._reg = _Font(reg_p)
            self._bold = _Font(bold_p) if bold_p else self._reg
            self._sub_n = _Subsetter(self._reg)
            self._sub_b = _Subsetter(self._bold)
        except _FontError:
            self._ttf = False

    # -------------------------------------------------- cizim yardimcilari
    def top(self, margin):
        return self.H - margin

    def lh(self, size):
        return int(size * 1.55) + 4

    def _text_op(self, x, y, s, style="n", size=10, align=None, ttf=None):
        """Tek metin cizim op'unu bayt olarak uretir (sayfaya eklemez).
        Akis modu (footer/baslik tekrari) da bu yolu kullanir."""
        if ttf is None:
            ttf = self._ttf
        s = str(s)
        if ttf:
            sub = self._sub_b if style == "b" else self._sub_n
            if align:
                w = sub.width_pt(s, size)
                if align == "right":
                    x -= w
                elif align == "center":
                    x -= w / 2.0
            fname = b"/F2" if style == "b" else b"/F1"
            hexcid = sub.encode(s)
            return (b"BT %s %s Tf 1 0 0 1 %.2f %.2f Tm <%s> Tj ET"
                    % (fname, ("%g" % size).encode("latin1"), x, y, hexcid))
        fname = b"/F2" if style == "b" else b"/F1"
        txt = s.encode("latin1", errors="replace").decode("latin1")
        txt = txt.replace("\\", r"\\").replace("(", r"\(").replace(")", r"\)")
        return (b"BT %s %s Tf 1 0 0 1 %.2f %.2f Tm (%s) Tj ET"
                % (fname, ("%g" % size).encode("latin1"), x, y,
                   txt.encode("latin1")))

    def text(self, x, y, s, style="n", size=10, align=None):
        self._ops.append(self._text_op(x, y, s, style, size, align))

    def hline(self, x1, y, x2, w=0.7):
        self._ops.append(b"%.2f w %.2f %.2f m %.2f %.2f l S"
                         % (w, x1, y, x2, y))

    def rect(self, x, y, w, h, lw=0.7):
        self._ops.append(b"%.2f w %.2f %.2f %.2f %.2f re S" % (lw, x, y, w, h))

    # ------------------------------------------------------------ build
    def _font_objects(self, w, font, subset, psname):
        program = _build_subset_program(font, subset.gids, subset.gid_pos)
        comp = zlib.compress(program, 9)
        widths = [font.glyph_advance(g) for g in subset.gids]
        w_array = b"[0 [" + " ".join(str(x) for x in widths).encode() + b"]]"
        ff = w.add(b"<< /Length %d /Length1 %d /Filter /FlateDecode >>\nstream\n"
                   % (len(comp), len(program)) + comp + b"\nendstream")
        fd = w.add(b"<< /Type /FontDescriptor /FontName /" + psname +
                   b" /Flags 4 /FontBBox [%d %d %d %d] /ItalicAngle 0 "
                   b"/Ascent %d /Descent %d /CapHeight 700 /StemV 80 "
                   b"/FontFile2 %d 0 R >>"
                   % (font.xMin, font.yMin, font.xMax, font.yMax,
                      font.ascent, font.descent, ff))
        tu_bytes = _tounicode_stream(subset.cid_char)
        tu = w.add(b"<< /Length %d >>\nstream\n" % len(tu_bytes)
                   + tu_bytes + b"\nendstream")
        cidf = w.add(b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /" +
                     psname +
                     b" /CIDSystemInfo << /Registry (Adobe) "
                     b"/Ordering (Identity) /Supplement 0 >> "
                     b"/FontDescriptor %d 0 R /DW 500 /W " % fd + w_array +
                     b" /CIDToGIDMap /Identity >>")
        top = w.add(b"<< /Type /Font /Subtype /Type0 /BaseFont /" + psname +
                    b" /Encoding /Identity-H /DescendantFonts [%d 0 R] "
                    b"/ToUnicode %d 0 R >>" % (cidf, tu))
        return top

    def _type1_objects(self, w):
        f1 = w.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
                   b"/Encoding /WinAnsiEncoding >>")
        f2 = w.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold "
                   b"/Encoding /WinAnsiEncoding >>")
        return f1, f2

    def build(self, baslik=None):
        if self._flow:
            return self._build_flow(baslik)
        w = _Writer()
        if self._ttf:
            f1 = self._font_objects(w, self._reg, self._sub_n, b"ZeytinSub")
            f2 = self._font_objects(w, self._bold, self._sub_b, b"ZeytinSubB")
        else:
            f1, f2 = self._type1_objects(w)
        res = w.add(b"<< /Font << /F1 %d 0 R /F2 %d 0 R >> "
                    b"/ProcSet [/PDF /Text] >>" % (f1, f2))
        content = b"\n".join(self._ops)
        compc = zlib.compress(content, 9)
        cont = w.add(b"<< /Length %d /Filter /FlateDecode >>\nstream\n"
                     % len(compc) + compc + b"\nendstream")
        page = w.add(b"<< /Type /Page /Parent %d 0 R "
                     b"/MediaBox [0 0 %.2f %.2f] /Resources %d 0 R "
                     b"/Contents %d 0 R >>" % (0, self.W, self.H, res, cont))
        pages = w.add(b"<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % page)
        # /Parent sonradan bilindigi icin page'i yeniden yazmak yerine
        # sayfa nesnesini Pages'tan SONRA olustur: yeniden duzenle
        w.objs[page - 1] = (b"<< /Type /Page /Parent %d 0 R "
                            b"/MediaBox [0 0 %.2f %.2f] /Resources %d 0 R "
                            b"/Contents %d 0 R >>"
                            % (pages, self.W, self.H, res, cont))
        cat = w.add(b"<< /Type /Catalog /Pages %d 0 R >>" % pages)
        title = baslik or "Zeytin Takip"
        info = w.add(b"<< /Producer (ZeytinTakip pdfgen) /Title " +
                     _utf16_hex(title) + b" >>")
        return w.build(cat, info)

    def _build_flow(self, baslik):
        """Cok sayfalı akis belgesi. Footer'lar (sayfa numaralari dahil) font
        nesnelerinden ONCE uretilir ki glifleri subset'e girebilsin."""
        # son (kirilmamis) sayfayi da flush et:
        if self._ops or not self._pages:
            self._pages.append(list(self._ops))
            self._ops = []
        total = len(self._pages)
        page_ops = []
        for i, ops in enumerate(self._pages):
            po = list(ops)
            po.extend(self._footer_ops(i + 1, total))
            page_ops.append(po)
        w = _Writer()
        if self._ttf:
            f1 = self._font_objects(w, self._reg, self._sub_n, b"ZeytinSub")
            f2 = self._font_objects(w, self._bold, self._sub_b, b"ZeytinSubB")
        else:
            f1, f2 = self._type1_objects(w)
        res = w.add(b"<< /Font << /F1 %d 0 R /F2 %d 0 R >> "
                    b"/ProcSet [/PDF /Text] >>" % (f1, f2))
        page_infos = []
        for po in page_ops:
            content = b"\n".join(po)
            compc = zlib.compress(content, 9)
            cont = w.add(b"<< /Length %d /Filter /FlateDecode >>\nstream\n"
                         % len(compc) + compc + b"\nendstream")
            page = w.add(b"<< /Type /Page /Parent 0 0 R "
                         b"/MediaBox [0 0 %.2f %.2f] /Resources %d 0 R "
                         b"/Contents %d 0 R >>" % (self.W, self.H, res, cont))
            page_infos.append((page, cont))
        kids = b" ".join(b"%d 0 R" % p for p, _ in page_infos)
        pages = w.add(b"<< /Type /Pages /Kids [" + kids + b"] /Count %d >>"
                      % len(page_infos))
        for page, cont in page_infos:
            w.objs[page - 1] = (b"<< /Type /Page /Parent %d 0 R "
                                b"/MediaBox [0 0 %.2f %.2f] /Resources %d 0 R "
                                b"/Contents %d 0 R >>"
                                % (pages, self.W, self.H, res, cont))
        cat = w.add(b"<< /Type /Catalog /Pages %d 0 R >>" % pages)
        title = baslik or "Zeytin Takip"
        info = w.add(b"<< /Producer (ZeytinTakip pdfgen) /Title " +
                     _utf16_hex(title) + b" >>")
        return w.build(cat, info)

    # ------------------------------------------ flowing document support
    def flow_begin(self, margin=40, bottom=46, footer=None, page_label=None):
        """Akis modunu baslatir. footer:
        None   -> varsayilan: sol baslik, sag 'Sayfa X / Y'
        callable(fn(page_num, total) -> str | None) -> sayfa basina cagrilir
        False  -> footer yok
        page_label: varsayilan footer'in sag etiketi ('Sayfa' yerine baska)
        """
        self._pages = []
        self._ops = []
        self._fm = float(margin)
        self._fb = float(bottom)
        self._cur = self.H - self._fm
        self._footer = footer
        self._plabel = page_label
        self._flow_title = None
        self._flow = True
        self._cols = []
        self._hdr_h = 0.0
        self._min_body = 0
        self._page_rows = []

    def _footer_ops(self, num, total):
        f = self._footer
        if f is False or self._fb >= self.H:
            return []
        y = self._fb
        left = right = None
        if f is None:
            if self._flow_title:
                left = self._text_op(self._fm, y, self._flow_title,
                                     style="n", size=8)
            lab = ("%s %d / %d" % (self._plabel, num, total)) \
                if self._plabel else ("Sayfa %d / %d" % (num, total))
            right = self._text_op(self.W - self._fm, y, lab,
                                  style="n", size=8, align="right",
                                  ttf=self._ttf)
        else:
            try:
                pair = f(num, total)
            except Exception:
                pair = None
            if isinstance(pair, (tuple, list)):
                if len(pair) >= 1:
                    left = pair[0]
                if len(pair) >= 2:
                    right = pair[1]
            elif pair is not None:
                left = pair
            if left:
                left = self._text_op(self._fm, y, left, style="n", size=8)
            if right:
                right = self._text_op(self.W - self._fm, y, right,
                                      style="n", size=8, align="right",
                                      ttf=self._ttf)
        ops = []
        if left:
            ops.append(left)
        if right:
            ops.append(right)
        return ops

    # ------------------------------------------------ tablo + akis metin API
    def flow_set_title(self, s):
        """Varsayilan footer'in sol metni (cagrilmasa da olur)."""
        self._flow_title = str(s)

    def flow_ensure(self, h):
        """h yukseklik kalmadiysa yeni sayfa ac (+ tekrar eden baslik)."""
        if self._cur - h >= self._fb:
            return
        if self._ops:
            self._flow_break()

    def _flow_break(self):
        self._pages.append(list(self._ops))
        self._ops = []
        self._cur = self.H - self._fm
        self._page_rows = []
        if self._cols:
            self._draw_header()          # tablo basligini tekrar et

    def flow_newpage(self):
        self._flow_break()

    def flow_text(self, x, y, s, style="n", size=10, align=None):
        """Mutlak koordinatli metin (gecerli sayfaya)."""
        self._ops.append(self._text_op(x, y, s, style, size, align))

    def flow_line(self, s, style="n", size=10, gap=None, indent=0.0,
                  ensure=True):
        """Sol kenardan baslayan tek satir metin; sigmadiysa once sayfa kirar."""
        h = self.lh(size)
        if ensure:
            self.flow_ensure(h + (gap or 0))
        self._cur -= h
        self._ops.append(self._text_op(self._fm + indent, self._cur, s,
                                       style=style, size=size))
        if gap:
            self._cur -= gap

    def table_header(self, columns, min_body=3):
        """Yeni tablo baslatir.
        columns: [(baslik, genislik, hizalama|None), ...]
        hizalama: 'left' | 'right' | 'center' | None (sol)
        # sutunu otomatik satir numarasidir (genisliklerden bagimsiz 16pt).
        Sutun basliklari sayfa kirilimlerinde OTOMATIK TEKRARLANIR.
        min_body: kirilim karari icin govdede en az kac satirlik yer tutulur
        """
        self._cols = list(columns)
        self._hdr_h = self.lh(9) + 6
        self._min_body = max(1, int(min_body))
        self._page_rows = []
        self._flow_ensure_table_header()

    def _flow_ensure_table_header(self):
        need = self._hdr_h + self._min_body * self.lh(10)
        if self._ops and self._cur - need < self._fb:
            self._flow_break()           # kirilim basligi kendisi cizer
        else:
            self._draw_header()

    def _draw_header(self):
        """Sutun basliklarini cizer (tablo baslangici ve her kirilimda)."""
        x0 = self._fm
        x1 = self.W - self._fm
        y_line = self._cur
        self._cur -= self._hdr_h
        self._ops.append(b"%.2f w %.2f %.2f m %.2f %.2f l S"
                         % (0.9, x0, y_line, x1, y_line))
        ty = self._cur + 4
        self._ops.append(self._text_op(x0, ty, "#", style="b", size=9))
        cx = x0 + 16
        for i, (title, wdt, align) in enumerate(self._cols):
            self._ops.append(self._text_op(cx, ty, str(title), style="b",
                                           size=9, align=align or "left"))
            cx += wdt
        self._ops.append(b"0.9 w %.2f %.2f m %.2f %.2f l S"
                         % (x0, self._cur, x1, self._cur))

    def table_row(self, cells, fill=None, size=10, h=None):
        """Bir govde satiri ekler. cells: sutun tanimlariyla eslesen metinler.
        fill: '#RRGGBB' verilirse satir arkaplani boyanir.
        Satir sigmadiysa once sayfa kirilir; sutun basliklari yeniden cizilir."""
        if not self._cols:
            raise RuntimeError("table_row: once table_header cagirin")
        hh = h if h is not None else self.lh(size)
        if self._cur - hh < self._fb:
            self._flow_break()
        elif (self._page_rows
              and self._cur - hh - self._min_body * self.lh(size) < self._fb):
            # bu satir sigsa bile arkasina min_body satir sigmayacaksa kir
            self._flow_break()
        self._row(cells, fill, size, hh)

    def _row(self, cells, fill, size, hh):
        if fill and self._ttf:
            rgb = tuple(int(fill[i:i + 2], 16) / 255.0 for i in (1, 3, 5))
            self._ops.append(b"%.2f %.2f %.2f rg %.2f %.2f %.2f %.2f re f"
                             % (rgb[0], rgb[1], rgb[2],
                                self._fm, self._cur - hh,
                                self.W - 2 * self._fm, hh))
        self._cur -= hh
        base = self._cur + (hh - size) / 2.0 + 1
        x0 = self._fm
        self._ops.append(self._text_op(x0, base, str(len(self._page_rows) + 1),
                                       size=size))
        cx = x0 + 16
        for i, (val, (title, wdt, align)) in enumerate(
                zip(cells, self._cols)):
            al = align or "left"
            if al == "right":
                tx = cx + wdt - 4
            elif al == "center":
                tx = cx + wdt / 2.0
            else:
                tx = cx
            self._ops.append(self._text_op(tx, base, str(val), size=size,
                                           align=al if al != "left" else None))
            cx += wdt
        self._page_rows.append(cells)

    def table_sep(self, w=0.5):
        """Satirlar arasi ince cizgi."""
        self._ops.append(b"%.2f w %.2f %.2f m %.2f %.2f l S"
                         % (w, self._fm, self._cur + 1,
                            self.W - self._fm, self._cur + 1))

    def table_end(self, toplam=None):
        """Tabloyu kapatir: alt cizgi; toplam metni verildiyse sag hizali
        TOPLAM satiri ekler (orn. table_end('3 adet / 1.234,50 TL'))."""
        x0 = self._fm
        x1 = self.W - self._fm
        self._ops.append(b"0.9 w %.2f %.2f m %.2f %.2f l S"
                         % (x0, self._cur + 2, x1, self._cur + 2))
        self._cur -= 4
        self._cols = []              # olasi kirilimda baslik tekrarlanmasin
        self._min_body = 0
        self._page_rows = []
        if toplam:
            self.flow_ensure(self.lh(10))
            self._cur -= self.lh(10)
            self._ops.append(self._text_op(x0, self._cur, "TOPLAM",
                                           style="b", size=10))
            self._ops.append(self._text_op(x1, self._cur, str(toplam),
                                           style="b", size=10,
                                           align="right"))
