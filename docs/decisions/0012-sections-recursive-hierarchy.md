# Sections / recursive hierarchy kararı

Tarih: 2026-09-23
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 18)

## Context

Projects (ADR 0011) sonrası hiyerarşinin ilk domain katmanı: bir Project
içinde **genel amaçlı, keyfi derinlikte** bölüm ağacı. "Blok", "Kat",
"Daire", "Bölge" gibi kavramlar VERİTABANI TİPİ DEĞİLDİR — yalnız kullanıcı
seçimi Section adlarıdır. Bu adım yalnızca Section katmanını kurar; Work
Items, Processes, timers, atamalar, ilerleme hesaplama kapsam dışıdır.

## Decision

1. **Tek genel recursive entity:** `sections` tablosu. Tip enum'u YOK
   (blok/floor/apartment kolonu yok); hiyerarşi düzeyleri kullanıcı adlarıyla
   ifade edilir. Sahte gizli "Project Root" satırı YOK — `parent_section_id
= NULL` kök anlamına gelir; bir projede 0..N kök olabilir.
2. **Adjacency-list:** `parent_section_id → sections.id`. Closure table /
   nested set / materialized path / ltree / derinlik önbelleği / ata dizisi
   BİLİNÇLİ OLARAK YOK — beklenen proje ölçekleri bunu gerekçelendirmiyor;
   ihtiyaç kanıtlanırsa sonraki aşamada değerlendirilir. Recursive CTE
   yalnızca döngü kontrolünde kullanılır.
3. **DB tenant/parent bütünlüğü (008):** `FOREIGN KEY (tenant_id,
workspace_id, project_id) → projects(tenant_id, workspace_id, id)` (bu
   hedef için projects'e `UNIQUE (tenant_id, workspace_id, id)` eklendi —
   004'teki workspaces deseni) ve **kompozit self-FK**
   `(tenant_id, workspace_id, project_id, parent_section_id) →
sections(tenant_id, workspace_id, project_id, id)`: depolanan bir ebeveyn
   her zaman çocuğuyla AYNI tenant/workspace/project üçlüsünü taşır —
   kiracılar-arası ve projeler-arası ebeveynlik DEPOLANAMAZ (test edildi).
4. **Döngü önleme (çekirdek invariant):** mutasyon transaction'ı içinde,
   recursive CTE (`UNION` — bozuk veride bile sonlanır) ile "hedef ebeveyn
   section'ın kendisi veya bir torunu mu" kontrolü; self-parent ayrıca
   reddedilir. Kontrol asla önbellek ağaç anlık görüntüsünde değil, her
   seferinde taze durumda çalışır. Hata semantiği: `VALIDATION_ERROR` +
   `fields.parent_section_id` (yeni hata kodu eklenmedi; §41 kararı).
5. **Concurrency / serileştirme:** her section mutasyonu transaction'ında
   ilgili **projects satırını `FOR UPDATE` ile kilitler** → aynı projenin
   tüm section mutasyonları serileşir → iki ters-yönlü taşıma (A↔B) aynı
   anda asla ikisi de başarılı olamaz, döngü kontrolü yarışsızdır
   (tokio::join testiyle kanıtlandı; dağıtık kilit yok).
6. **Sibling sıralama:** integer `position`. `(parent, position)` UNIQUE
   DEĞİL (bilinçli, §11 değerlendirmesi): strict unique her araya eklemede
   kardeş grubunun tamamını kaydırma zorlardı; sıralama sözleşmesi
   `(position, id)` ile deterministiktir, eşzamanlı append'ler bozulma
   yaratamaz. Create: ebeveyn grubunda `max+1` (append). PATCH position:
   açık değer. Drag/drop ve future bulk generation aynı primitive'ı kullanır.
   **Bilinen sonuç:** "position 0 ver" tek başına ilk sırayı garantilemez
   (eşitlikte id belirler) — gerçek yeniden sıralama kardeş grubunun
   pozisyonlarını birlikte ayarlar (UI'daki Yukarı/Aşağı ve testler böyle
   çalışır).
7. **Slug semantiği:** mevcut `slug_from_text`/`validate_slug` yeniden
   kullanılır. Teklik **kardeş kapsamında**: aynı ebeveyn altında
   case-insensitive (citext) hard; farklı ebeveynler altında aynı slug serbest
   ("Kat 1/Daire 1" ve "Kat 2/Daire 1"). PostgreSQL NULL-semantiği nedeniyle
   kökler ve çocuklar ayrı **partial unique index**'lerle (005 membership_roles
   deseni). Soft-delete slug'ı serbest bırakmaz. Kimlik = UUID; slug route
   otoritesi değil.
8. **Taşıma (reparent) semantiği:** `sections:update` yetkisiyle PATCH
   (`parent_section_id` üç durumlu: yok = korunur, `null` = kök, UUID =
   taşıma; serde double-option). Transaction'da yeniden kanıtlanır: üyelikler
   → project satır kilidi → izin → section satır kilidi → hedef ebeveyn AYNI
   proje sınırında taze çözüm + kilit (silinmiş/arşivlenmiş ebeveyn reddi) →
   döngü CTE → geçiş kontrolü. Taşıma pozisyonsuz gelirse yeni kardeş
   grubunda append olur; açık position geçerli olur. Alt ağaç mantıksal
   olarak taşınır (torun satırları hiç yazılmaz — test edildi); projeler-arası
   taşıma desteklenmez (aynı-proje dışı ebeveyn = invalid).
9. **Lifecycle:** yapısal `active | archived` (DB CHECK). Sections "tamamlanmaz"
   — completed durumu yok. Arşivleme yalnızca `status:"archived"` SET eden
   PATCH'te ek `sections:archive` ister; unarchive update yeterli. Arşivli
   sectionlar listede KALIR (işaretli; "Daire 1 — Arşivlendi") — hiçbir şey
   gizlenmez, archived bir durumdur; böylece gizli-ebevey/torun-sızıntısı
   sorunu tasarım gereği yoktur. Arşivli ebeveynin altına taşıma reddedilir.
   `deleted_at` yalnızca şema altyapısıdır; DELETE endpoint YOK (ürün
   semantiği tanımlanana dek ertelendi); silinmiş satırlar okuma
   yollarından düşer (uniform 404 — parite testli).
10. **İlerleme alanı YOK:** progress/percent/cached alanları bilinçli olarak
    yasak (§17) — Sections henüz iş taşımaz; gelecekte Work Items/Processes
    gerçek semantikle gelir.
11. **Project authority zinciri:** `ProjectContext` (ADR 0011) sınır olarak
    kalır; koleksiyon rotaları (`GET/POST .../projects/{p}/sections`)
    ProjectContext, tekil rotalar yeni `SectionContext` (named-struct
    `SectionRoute`, 4 alan; 5-parametreli gelecek `.../work-items/{w}`
    rotasında çalıştığı http.rs pin testiyle sabitlendi) kullanır: auth →
    UUID parse (malformed == unknown) → `workspaces::find_accessible` →
    `projects::find_accessible` → `sections::find_accessible` (id ∧ proje ∧
    ws ∧ tenant ∧ deleted_at NULL tek sorgu). İkinci bir yetki modeli yok;
    401 → 404 (eligibility/yabancı/yanlış-ebeveyn/silinmiş, hepsi uniform,
    fingerprint-parite testli) → 403 (eligible-yetkisiz mutasyon).
12. **Read politikası:** eligibility-based (`sections:read` YOK — Projects
    politikasının devamı; görebilen üye okur).
13. **İzinler:** `sections:create`, `sections:update` (taşıma + yeniden
    sıralama dahil — ayrı move anahtarı bilinçli olarak yok: tek enforcement
    yolu, V1'de granülarite gerekçesi yok), `sections:archive`. Owner
    bootstrap listesi ve 008 migration backfill'i mevcut Owner ROLLERİNE
    grant verir; membership_roles backfill / Owner rol ataması YOK (007
    deseni; test edildi).
14. **Ağaç yanıt şekli — FLAT:** `GET .../sections` tek sorguda düz,
    deterministik sıralı liste döndürür (`ORDER BY parent_section_id NULLS
FIRST, position, id`; kökler önce, çocuklar ebeveyn-id gruplarında
    (position, id) ile). İç içe recursive DTO bilinçli olarak seçilmedi:
    OpenAPI/codegen ergonomisi, Svelte render basitliği ve gelecekteki
    taşıma/yeniden sıralama karararlılığı kazanç. İstemci ağacı bellekte
    kurar.
15. **Sorgu stratejisi / N+1:** ağaç getirme = **1 SQL** (kasıtlı tek
    `query_as`); recursive uygulama-seviyeli yürüyüş yok. 165 sectionluk
    gerçekçi hiyerarşi (15 kat × 10 daire) tek API çağrısında tam ve
    deterministik döner (test edildi) — future bulk generation (15 kat × 10
    daire = 165 NORMAL satır) şema değişikliği gerektirmez; üretec şimdi
    YOK.
16. **TOCTOU:** create/update transaction'ı şunları taze kanıtlar: aktif org
    - ws üyeliği (`workspaces::find_accessible` tx snapshot'ında), project
      var/silinmemiş (satır kilidi), izin (`authorize_workspace_in_tx`),
      ebeveyn geçerliliği (taze çözüm + `FOR UPDATE`), döngü invariant'ı.
      Deterministik testler: ws/org üyelik iptali, grant iptali, atama
      iptali, ebeveyn-arşivleme yarışı — hepsi 0 yetkisiz satır yazımıyla.
17. **Frontend:** proje detay sayfası bölümler panelini barındırır: düz
    liste + **güvenli bellek-içi ağaç kurucu** (`$lib/sections/tree.ts` —
    dup/orphan/döngü-bozuk-girdi toleransı, birim testli; yetki DEĞİL
    sunum), derinliğe göre girinti, boş durum, kök ekleme formu, satır içi
    alt-bölüm ekleme/yeniden adlandırma, Taşı (ebeveyn select; kendisi ve
    torunları hariç — backend yine otoriter), Köke taşı, Yukarı/Aşağı
    (position primitive), Arşivle/Yeniden etkinleştir — hepsi ws-seviye
    effective-permissions'a göre izin-farkındalıklı (UX only). i18n tr/en;
    mevcut design token'ları. Ayrı section detay rotası V1'de yok (ağaç
    içinde yönetim); proje derin bağlantısı SSR + reload korunur (E2E).
18. **Gelecek uyumluluk:** `sections.id` altına Work Items (ve onların
    Processes'leri) ekleneğe hazır: `/.../sections/{s}/work-items/{w}` rotası
    pin testiyle güvence altında; bağımlılıklar, timers, atamalar ve ilerleme
    toplama bu şemayı yeniden tasarlama gerektirmeden eklenebilir.

## Non-goals (bu adımda bilinçli olarak YOK)

Work Items / İşçilik, Processes, process groups, dependencies, timers,
teams, assignments, ilerleme yüzdeleri, QR, TV, cards domain, bulk
bölüm üreteci, bölüm/proje şablonları, drag-and-drop kütüphanesi,
websocket/canlı güncelleme, audit/outbox, arama, metadata/property sistemi,
DELETE/restore endpoint'i, starter senkronizasyonu.

## Consequences

- 008, projects tablosuna FK-hedefi olarak `UNIQUE (tenant_id, workspace_id,
id)` ekler (id zaten PK; yalnızca kompozit FK hedefi — 004 workspaces
  deseni); down bu kısıtı kaldırır.
- Section mutasyonlarının projects satır kilidi üzerinden serileşmesi,
  tek-proje ölçeğinde mutasyon hızını sınırlar (bilinçli takas: doğruluk ve
  yarışsız döngü kontrolü öncelikli; V1 ölçeklerinde kabul edilebilir).
- Düz liste sözleşmesi istemcileri ağaç kurmaya zorlar; sıralama sözleşmesi
  belgelidir ve testlerle sabitlenmiştir.
- E2E ortamına dördüncü seed kullanıcı (e2e4@example.test) eklendi; spec
  dosyaları kullanıcı başına izole kalır.
