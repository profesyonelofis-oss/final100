// KARAOGLU Zeytin Takip - web JS
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
});
