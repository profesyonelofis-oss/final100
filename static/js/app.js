// KARAOGLU Zeytin Takip - web JS

/* IBAN kopyalama: banner ve uyelik sayfasindaki Kopyala butonlari */
function ibanKopyala(elementId, btn) {
  var el = document.getElementById(elementId);
  if (!el) return;
  var metin = (el.textContent || "").trim().replace(/\s+/g, "");
  function basari() {
    if (!btn) return;
    var eski = btn.textContent;
    btn.textContent = "✓ Kopyalandı";
    setTimeout(function () { btn.textContent = eski; }, 2000);
  }
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(metin).then(basari).catch(function () {
      _ibanKopyalaYedek(metin, basari);
    });
  } else {
    _ibanKopyalaYedek(metin, basari);
  }
}
function _ibanKopyalaYedek(metin, basari) {
  /* Eski tarayicilar / https olmadan calisan yedek yontem */
  var ta = document.createElement("textarea");
  ta.value = metin;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  try { document.execCommand("copy"); basari(); } catch (e) {}
  document.body.removeChild(ta);
}

document.addEventListener("DOMContentLoaded", function () {
  // Canli saat
  var clock = document.getElementById("clock");
  if (clock) {
    var tick = function () {
      var d = new Date();
      clock.textContent = d.toLocaleTimeString("tr-TR");
    };
    tick();
    setInterval(tick, 1000);
  }

  // Alim / teslimat fisini canli hesapla
  function hesapla() {
    var toplamKg = 0, toplamTutar = 0;
    document.querySelectorAll("[data-kg]").forEach(function (inp) {
      var kalibre = inp.getAttribute("data-kg");
      var kg = parseFloat((inp.value || "").replace(",", ".")) || 0;
      var fiyatInp = document.querySelector('[data-fiyat="' + kalibre + '"]');
      var fiyat = parseFloat((fiyatInp && fiyatInp.value || "").replace(",", ".")) || 0;
      var satirToplam = kg * fiyat;
      toplamKg += kg;
      toplamTutar += satirToplam;
      var hucre = document.querySelector('[data-toplam="' + kalibre + '"]');
      if (hucre) {
        hucre.textContent = satirToplam > 0
          ? satirToplam.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + " ₺"
          : "—";
      }
    });
    var kgEl = document.getElementById("toplam-kg");
    var tutarEl = document.getElementById("hesaplanan-tutar");
    if (kgEl) kgEl.textContent = toplamKg.toLocaleString("tr-TR", { minimumFractionDigits: 3, maximumFractionDigits: 3 }) + " KG";
    if (tutarEl) tutarEl.textContent = toplamTutar.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + " ₺";
    var komisyon = parseFloat((document.getElementById("komisyon-deger") || {}).value || 0);
    var odenecek = toplamTutar - toplamKg * komisyon;
    var odenecekEl = document.getElementById("odenecek-tutar");
    if (odenecekEl) odenecekEl.textContent = odenecek.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + " ₺";
  }

  document.querySelectorAll("[data-kg],[data-fiyat]").forEach(function (inp) {
    inp.addEventListener("input", hesapla);
  });
  hesapla();

  /* ---------- Fiyat kilitleme (cesit bazli, alim fisi) ----------
     Secili zeytin cesidine gore fiyat kilidi:
       - Trilya secili + fiyatlar yazili + 🔒 Kilitle -> fiyatlar TRILYA'ya
         kaydedilir ve kilitlenir.
       - Edremit radio'suna tiklaninca Edremit'in kendi fiyatlari (varsa)
         yuklenir; Edremit kilitsizse genis fiyat kalir, istenirse
         ayri fiyat girip Edremit'e de kilitlemek mumkundur.
       - Kilite durumu sunucuda saklanir; telefon/tablet/bilgisayarda
         ve yeni musterilerde ayni fiyatlardan devam edilir. */
  var kilitBtn = document.getElementById("fiyat-kilit-btn");
  if (kilitBtn) {
    var fiyats = Array.prototype.slice.call(document.querySelectorAll("[data-fiyat]"));
    var cesitRadios = document.querySelectorAll('input[name="cesit"]');
    var ozetAciklama = document.getElementById("kilit-aciklama");

    function seciliCesit() {
      var r = document.querySelector('input[name="cesit"]:checked');
      return r ? r.value : "";
    }

    function fiyatlariKilitle(kilitli) {
      fiyats.forEach(function (inp) {
        inp.readOnly = kilitli;
        inp.classList.toggle("fiyat-kilitli", kilitli);
      });
      var c = seciliCesit();
      kilitBtn.textContent = kilitli
        ? "🔓 KİLİDİ AÇ" + (c ? " — " + c : "")
        : "🔒 BU ÇEŞİT FİYATINI KİLİTLE" + (c ? " — " + c : "");
      kilitBtn.classList.toggle("notor", kilitli);
      kilitBtn.setAttribute("aria-pressed", kilitli ? "true" : "false");
    }

    function fiyatlarDoluMu() {
      return fiyats.some(function (inp) {
        return parseFloat((inp.value || "").replace(",", ".")) > 0;
      });
    }

    kilitBtn.addEventListener("click", function () {
      var kilitli = fiyats.length && fiyats[0].readOnly;
      if (!kilitli) {
        if (!seciliCesit()) {
          alert("Önce zeytin çeşidi seçin; kilit çeşide göre kaydedilir.");
          return;
        }
        if (!fiyatlarDoluMu()) {
          alert("Önce fiyat girin; boş fiyatlar kilitlenmez.");
          return;
        }
        fiyatlariKilitle(true);
        kilitBtn.disabled = true;
        kilitBtn.textContent = "⏳ KİLİTLENİYOR...";
        var gonderilecek = { cesit: seciliCesit() };
        fiyats.forEach(function (inp) {
          gonderilecek["fiyat_" + inp.getAttribute("data-fiyat")] = inp.value || "0";
        });
        fetch("/fiyat-kilitle", {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: new URLSearchParams(gonderilecek).toString()
        }).then(function (r) { return r.json(); })
          .then(function (j) {
            kilitBtn.disabled = false;
            fiyatlariKilitle(!!j.ok);
          })
          .catch(function () { kilitBtn.disabled = false; fiyatlariKilitle(false); });
      } else {
        fiyatlariKilitle(false);
      }
    });

    /* Cesit radio'su degisir: o cesidin fiyatlari sunucudan yuklenir.
       Cesidin kendi kaydi yoksa genel kalibre fiyatlari listelenir. */
    cesitRadios.forEach(function (radio) {
      radio.addEventListener("change", function () {
        var c = radio.value;
        fetch("/cesit-fiyatlari?cesit=" + encodeURIComponent(c))
          .then(function (r) { return r.json(); })
          .then(function (j) {
            if (!j.ok) return;
            fiyats.forEach(function (inp) {
              var k = inp.getAttribute("data-fiyat");
              inp.value = (j.fiyatlar[k] && j.fiyatlar[k] > 0)
                ? String(j.fiyatlar[k]).replace(".", ",") : "";
            });
            fiyatlariKilitle(!!j.kilitli);
            hesapla();
          })
          .catch(function () { fiyatlariKilitle(false); });
      });
    });

    /* Acilista: sayfa secili cesitle yuklendi (cesit_kilitli bilgisi) */
    if (fiyatlarDoluMu() && kilitBtn.getAttribute("aria-pressed") !== "unknown") {
      fiyatlariKilitle(document.body.getAttribute("data-cesit-kilitli") === "1");
    }
  }
});
