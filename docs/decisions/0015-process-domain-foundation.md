# Process domain foundation (süreç tanımları)

Tarih: 2026-09-24
Durum: Uygulandı; local verification / review milestone (commit edilmedi)

## Model ve sınır

Process (TR: Süreç), tam olarak bir organization → workspace → project →
section → work item zincirine ait, sıralı bir **tanımdır** (definition).
Örnek: "Mutfak Tezgahı" işçiliği altında 01 Taş Alımı, 02 Kesim, 03 İmalat,
04 Nakliye, 05 Montaj. STEP 20 yalnız hangi süreçlerin var olduğunu, sırasını
ve zorunlu/opsiyonel olduğunu yapılandırır.

**Definition / execution sınırı (kritik kabul kriteri).** `processes` satırı
çalışma zamanı durumu taşımaz: `started_at`, `finished_at`, süre, timer,
atanan kişi/ekip, yürütme durumu veya ilerleme yüzdesi YOKTUR. Gelecekteki
yürütme (başlat/bitir/reddet/geri al, timer session'ları, atamalar) ayrı
kayıtlar olarak tasarlanacak ve kararlı `processes.id`'ye referans verecek;
tanım satırı mutable runtime state ile aşırı yüklenmeyecek. Bu sayede bir
tanımın adı/sırası değişse de geçmiş yürütme kayıtları kimliğini korur.
Integration testi tabloda yürütme kolonlarının bulunmadığını doğrular.

Kavramsal `DATABASE_SCHEMA.md` taslağı Process'i `cards` altında,
`process_steps` ile ve `system_status`/`revision`/`estimated_duration`
kolonlarıyla tarif ediyordu. STEP 19 `cards`'ı `work_items` ile değiştirdi
(ADR 0013) ve STEP 20 talimatı tek seviyeli tanımı açıkça istedi; bu nedenle
Process, Work Item'ın çocuğudur, `process_steps` ve execution kolonları
eklenmedi. Sapma burada ve schema dokümanında kayıt altındadır.

## Persistence / migration 010

`20260924100000_processes`. Önceki dokuz migration değiştirilmedi.

- `work_items` üzerine composite FK hedefi:
  `UNIQUE (tenant_id, workspace_id, project_id, section_id, id)`.
- `processes`: UUIDv7 id; tenant_id, workspace_id, project_id, section_id,
  work_item_id (hepsi NOT NULL); name (1–200), citext slug (1–64),
  description NULL (≤2000), position ≥ 0, `is_required boolean DEFAULT true`,
  status `active|archived`, created_at, updated_at, deleted_at.
- Beş kolonlu `processes_work_item_fk` → `work_items(tenant, workspace,
project, section, id)`. work_items'ın section FK'si ve sections'ın project
  FK'si ile zincirin tamamı DB tarafından korunur: yanlış tenant, workspace,
  project, section veya work item birleşimi depolanamaz (doğrudan SQL
  testleri 23503 ile doğrular).
- `UNIQUE (work_item_id, slug)` hard ve case-insensitive; archive/soft delete
  slug'ı serbest bırakmaz. Aynı slug başka work item'da kullanılabilir.
- Kısmi unique index `processes_active_position_unique (work_item_id,
position) WHERE deleted_at IS NULL AND status = 'active'`: iki aktif süreç
  aynı sırayı paylaşamaz. Sections/Work Items'tan farklı olarak burada sıra
  bir **domain gerçeğidir** (01…05), bu yüzden DB seviyesinde zorlanır.
  Arşivli satırlar indeksten çıkar ve tarihsel pozisyonlarını korur.
- Sıralama indeksi `(work_item_id, position, id)`.
- İzinler: `processes:create`, `processes:update`, `processes:archive`,
  `processes:reorder`. Backfill yalnız mevcut system Owner rollerine
  organization-scope grant ekler; `membership_roles`'a yazmaz, rol atamaz,
  eski atamaları diriltmez. Member izin kazanmaz. Yeni organizasyon
  bootstrap'i aynı dört anahtarı açık listeden verir.
- Down: tabloyu, grant'leri, catalog satırlarını, son olarak work_items FK
  hedefini kaldırır. CASCADE yok; bağımlı nesne rollback'i engeller.

## Sıralama ve reorder

Kanonik sıra `position ASC, id ASC`. Create, iş öğesindeki **tüm** satırların
(arşivli dahil) max+1 değerine ekler; böylece yeni aktif süreç hiçbir aktif
pozisyonla çakışamaz. Bigint ara değer taşmayı validation hatasına çevirir.
PATCH `position` kabul etmez: sıra yalnız reorder komutuyla değişir.

`PATCH .../processes/reorder` gövdesi `{process_ids: uuid[]}` — iş öğesinin
TÜM aktif süreçleri, her biri tam bir kez. Sunucu, kilitli aktif kümeyle
karşılaştırır; eksik, fazla, tekrar eden, bilinmeyen, yabancı (başka work
item/tenant) veya arşivli id tek bir `process_ids` 422 hatası üretir (varlık
oracle'ı yok). Yazma iki fazlıdır: önce tüm satırlar mevcut maksimumun
üstüne (max+1…), sonra 0…n-1'e taşınır; n aktif farklı pozisyon için
max ≥ n-1 olduğundan hiçbir ara durum kısmi unique index'i ihlal etmez.
Mutation testi, ilk faz kaldırıldığında reorder testlerinin düştüğünü
doğruladı. Boş iş öğesinde boş liste geçerli no-op'tur. Aynı dizi gelecekte
drag-and-drop UI tarafından da kullanılabilir.

Route: statik `reorder` segmenti `{process_id}` parametre rotasından önce
eşleşir (axum/matchit önceliği); `GET .../reorder` 405 döner, gerçek
process rotaları etkilenmez (test edildi).

## Zorunlu / opsiyonel

`is_required` yalnız saklanan bir yapılandırma gerçeğidir. STEP 20 tamamlama
zorlaması veya ilerleme hesabı yapmaz. Gelecekte ilerleme, **yürütme
kayıtları + zorunlu/opsiyonel tanımlardan türetilecek**; Project/Section/
Work Item üzerinde elle saklanan yüzde alanı eklenmeyecek.

## Durum ve arşiv

Status bir **yapılandırma** yaşam döngüsüdür: `active | archived`.
`pending/running/completed/failed` yürütme kavramlarıdır ve reddedilir (422).
Arşiv mevcut repo konvansiyonuyla PATCH `status: "archived"` ile yapılır;
`processes:update` + `processes:archive` gerektirir (archive tek başına
yetmez, test edildi). Arşiv terminaldir: satır kalır, list/get/update/reorder
dışında kalır, slug dolu kalır; arşivli, bilinmeyen, yabancı ve yanlış
parent süreç aynı generic 404 parmak izini üretir. Restore/Undo/trash bu
adımda yoktur; UI onay metni bunu açıkça söyler.

## Context / routing / RBAC

Prefix: `/api/v1/organizations/{o}/workspaces/{w}/projects/{p}/sections/{s}/work-items/{i}/processes`.
Koleksiyon rotaları mevcut `WorkItemContext`'i (tam zincir + aktif section +
arşivsiz project + görünür work item) kullanır. `ProcessContext` bunun
üzerine süreci tam scope ile çözer; global `/processes/{id}` yoktur.
Named path struct extraction; gerçek context daha derin bir test rotasında
(`.../processes/{id}/executions/{x}`) doğrulanır. Cookie'ler yetki girdisi
değildir.

Reads eligibility-based (iki aktif üyelik + tam zincir). Mutations permission
key tabanlıdır: create → `processes:create`; PATCH → `processes:update`
(+ archive için `processes:archive`); reorder → yalnız `processes:reorder`
(update reorder'ı ima etmez, reorder update'i ima etmez). Rol adı
authorization'da kullanılmaz. Workspace-scope grant başka workspace'e
taşmaz. Frontend kontrolleri effective permissions ile gizler; backend
bağımsız olarak 403 döner (E2E doğrular).

Hata sırası: 401 → 404 → 403 → 422. JSON/type/unknown-field hataları mevcut
ApiJson konvansiyonuyla 400 VALIDATION_ERROR; temel izin kontrolü gövde
hatasından önce çalışır (yetkisiz kullanıcı bozuk gövdeyle de 403 alır).
DTO'lar `deny_unknown_fields`: tenant/organization/workspace/project/
section/work_item/user/role id, position, yürütme alanları reddedilir.
Raw SQL hatası response'a aktarılmaz.

## Transaction / kilit sırası / eşzamanlılık

Work Items sırası (ADR 0013) önekiyle genişletildi, değiştirilmedi:

1. project FOR UPDATE (proje içi tüm yazmaları serileştirir),
2. direct section + org + workspace + iki üyelik FOR SHARE,
3. work item FOR SHARE (arşivli/silinmiş work item altında yazma yok),
4. permission destek satırları FOR SHARE (`lock_workspace_permission_in_tx`),
5. process satır(lar)ı FOR UPDATE.

Work item update'i de 1→2→work item FOR UPDATE sırasını izler; her iki yol
önce project kilidini aldığından deadlock oluşmaz. Önceden commit edilmiş
üyelik/izin/atama iptali, project/section/work item arşivi reddedilir;
doğrulamadan sonra yarışan iptal commit'e kadar bekler. Deterministik TOCTOU
testleri (sleep yok) create/update/archive/reorder için bunları kanıtlar.

Eşzamanlı aynı slug → tam bir kazanan (diğeri `slug` 422). Eşzamanlı beş
append → 0…4 farklı pozisyon. Eşzamanlı iki reorder → ikisi de serileşir,
son durum ikisinden biri ve 0…n-1. Reorder ile append yarışında reorder ya
başarılı olur ya stale küme 422 alır; son küme eksiksiz ve sıralıdır.

Revision/optimistic concurrency eklenmedi (mevcut Projects/Sections/Work
Items modeli): aynı alana son başarılı yazma kazanır. Stale reorder ise
küme doğrulaması sayesinde sessizce ezilmez; UI yenileme ister.

## Gelecek uyumluluğu

- **Process Groups/şablonlar:** bir grup uygulandığında bu tabloya normal
  satırlar olarak kopyalanabilir (tanım kimliği korunur). Somut ihtiyaç
  olmadan nullable template/group id eklenmedi; kaynak izleme gerekirse
  ayrı, versiyonlu bir ilişkiyle eklenecek.
- **Yürütme:** ayrı `process_executions`/session kayıtları `processes.id`'ye
  referans verir; timer'lar saniyelik yazma yapmaz (AGENTS.md).
- **Bağımlılık/önkoşul, atama, realtime/outbox, audit, TV:** bu adımda yok.
- **İlerleme:** yalnız gerçek yürütme verisinden türetilir.

## Frontend

Work Item detay sayfasına "Süreçler" bölümü eklendi (shell, drill-down ve
STEP 19.5D kararları değişmedi). Numaralı `<ol>` satırları yalnız gerçek
veri gösterir: sıra, ad, açıklama, Zorunlu/Opsiyonel rozeti (metin + ●/○
işareti, yalnız renk değil). Yürütme UI'ı (yüzde, timer, başlat/bitir,
atanan) yoktur. Oluşturma/düzenleme FormDrawer'da; arşiv ConfirmDialog ile;
satır işlemleri ActionMenu'de. Sıralama için yeni bağımlılık eklenmedi:
erişilebilir Yukarı/Aşağı butonları (etiketli, tooltip'li, 44px), taşıma
sonrası odak aynı satırda kalır ve polite live region duyurur. Kontroller
hydration öncesi devre dışıdır. Aktif süreçler listelendiği için satırda
ayrıca "Aktif" durum rozeti gösterilmez (gereksiz tekrar).

## Doğrulama kapsamı

Gerçek PostgreSQL suite `tests/processes.rs`: CRUD/varsayılanlar, validation
ve scope injection, 401/404/403/422, wrong-parent + enumeration parity,
composite FK, aktif pozisyon tekliği, slug ad alanı, append/reorder, zorunlu/
opsiyonel, izin ayrıştırması, TOCTOU (üyelik, izin, atama, project/section/
work item/process), yaşam döngüsü kapıları, eşzamanlılık, hostile cookie,
workspace scope, derin rota context'i, backfill ve rollback güvenliği.
Browser: owner tanımla/düzenle/sırala/arşivle + reload, mobil/koyu/EN,
yetkisiz Member UI + backend 403. Sayılar milestone raporundadır.
