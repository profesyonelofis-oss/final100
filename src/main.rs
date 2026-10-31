use std::fs;
use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct BusinessLead {
    name: String,
    address: String,
    category: String,
    has_website: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Otomatik Git/Push Entegrasyonlu Demo Motoru ---");

    if !Path::new("demos").exists() {
        fs::create_dir("demos")?;
    }

    let raw_leads = vec![
        BusinessLead {
            name: "Boğaziçi Restoran".to_string(),
            address: "Beşiktaş, İstanbul".to_string(),
            category: "Restoran / Kafe".to_string(),
            has_website: false,
        },
        BusinessLead {
            name: "Tarihi Lezzet Lokantası".to_string(),
            address: "Kadıköy, İstanbul".to_string(),
            category: "Restoran / Kafe".to_string(),
            has_website: false,
        },
    ];

    let prospects: Vec<&BusinessLead> = raw_leads.iter().filter(|b| !b.has_website).collect();

    for (index, lead) in prospects.iter().enumerate() {
        let variant_id = index % 2; 
        
        let html_content = generate_protected_html(&lead.name, &lead.address, &lead.category, variant_id);

        let file_slug = lead.name.to_lowercase()
            .replace(" ", "_")
            .replace("ç", "c").replace("ğ", "g").replace("ı", "i")
            .replace("ö", "o").replace("ş", "s").replace("ü", "u");
            
        let file_name = format!("demos/{}.html", file_slug);
        fs::write(&file_name, &html_content)?;
        
        let message = generate_unique_message(&lead.name, &lead.address, &lead.category, &file_slug, index);
        
        println!("--------------------------------------------------");
        println!("  -> Korumalı Demo Üretildi: {}", file_name);
        println!("  -> Size Özel WhatsApp Teklif Metni:\n{}", message);
    }

    // OTO-YÜKLEME (GIT PUSH) ADIMI
    println!("\n[>] Demolar GitHub'a yükleniyor...");
    
    let status_add = Command::new("git").args(["add", "demos/"]).status()?;
    if status_add.success() {
        let status_commit = Command::new("git").args(["commit", "-m", "Otomatik yeni demo siteleri eklendi"]).status();
        
        if status_commit.is_ok() {
            let status_push = Command::new("git").args(["push", "origin", "main"]).status()?;
            if status_push.success() {
                println!("[✓] Başarıyla GitHub'a gönderildi! Canlı linkleriniz aktifleşiyor.");
            } else {
                println!("[!] Push sırasında bir hata oluştu.");
            }
        } else {
            println!("[i] Gönderilecek yeni değişiklik bulunamadı (zaten güncel).");
        }
    }

    Ok(())
}

fn generate_protected_html(name: &str, address: &str, category: &str, _variant: usize) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="tr">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{name} - Özel Taslak Demo</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-zinc-950 text-zinc-100 font-sans">

    <div id="demo-banner" class="bg-amber-600 text-black text-xs font-bold text-center py-2 px-4">
        ⚠️ BU SAYFA {name} İÇİN ÖZEL HAZIRLANMIŞ 12 SAAT GEÇERLİ TASLAK DEMODUR.
    </div>

    <div id="main-content" class="max-w-4xl mx-auto text-center py-20 px-4">
        <h1 class="text-5xl font-black mb-4">{name}</h1>
        <p class="text-amber-400 mb-6">{address}</p>
        <p class="text-zinc-400 mb-8">{category} sektörüne özel modern ve hızlı web altyapısı.</p>
        <a href="tel:05000000000" class="bg-amber-500 text-zinc-950 font-bold py-3 px-8 rounded-full inline-block">Hemen Bilgi Al</a>
    </div>

    <script>
        const fileSlug = "{file_slug}";
        let demoStart = localStorage.getItem("demo_start_" + fileSlug);
        
        if (!demoStart) {{
            demoStart = new Date().getTime();
            localStorage.setItem("demo_start_" + fileSlug, demoStart);
        }}

        const now = new Date().getTime();
        const twelveHoursInMs = 12 * 60 * 60 * 1000;

        if ((now - parseInt(demoStart)) > twelveHoursInMs) {{
            document.body.innerHTML = `
                <div style="height:100vh; display:flex; flex-direction:column; justify-content:center; align-items:center; background:#18181b; color:white; font-family:sans-serif; text-align:center; padding:20px;">
                    <h1 style="color:#ef4444; font-size:2rem; margin-bottom:10px;">12 Saatlik Demo Süresi Sona Erdi</h1>
                    <p style="color:#a1a1aa; max-width:400px; margin-bottom:20px;">Bu işletme için hazırlanan taslak çalışmanın erişim süresi dolmuştur. Web sitesini kalıcı olarak yayına almak için lütfen iletişime geçin.</p>
                    <a href="tel:05000000000" style="background:#f59e0b; color:black; padding:10px 20px; border-radius:9999px; text-decoration:none; font-weight:bold;">İletişime Geç</a>
                </div>
            `;
        }}
    </script>
</body>
</html>"#,
        name = name,
        address = address,
        category = category,
        file_slug = name.to_lowercase().replace(" ", "_")
    )
}

fn generate_unique_message(name: &str, address: &str, category: &str, file_slug: &str, index: usize) -> String {
    let demo_link = format!("https://KULLANICI_ADINIZ.github.io/REPO_ADINIZ/{}.html", file_slug);
    
    match index % 4 {
        0 => format!(
            "Selamlar {} ekibi, {} konumundaki yerinizi fark ettim ancak internette resmi sitenizi göremedim. Müşterilerinizin sizi daha kolay bulabilmesi adına {} sektörüne uygun ufak bir web sitesi taslağı hazırladım, göz atmak isterseniz: {}",
            name, address, category, demo_link
        ),
        1 => format!(
            "İyi günler dilerim, {} (@ {}). Bölgedeki aramalarda web sitenizin eksik olduğunu fark edince sizin için modern bir altyapı tasarladım. Hazırladığım önizleme linki burada, incelemek isterseniz sevinirim: {}",
            name, address, demo_link
        ),
        2 => format!(
            "Merhaba, {} işletmeniz için dijital dünyada işinizi kolaylaştıracak bir adım attım. Aktif siteniz bulunmadığı için size özel hazırladığım profesyonel demo sayfasına şu adresten bakabilirsiniz: {}",
            name, demo_link
        ),
        _ => format!(
            "Hey {} yetkilisi, {} bölgesinde harika işler yapıyorsunuz fakat dijitalde web siteniz eksik kalmış. Sektörünüze özel hazırladığım şu modern taslak demo siteye bir göz atın derim: {}",
            name, address, demo_link
        ),
    }
}