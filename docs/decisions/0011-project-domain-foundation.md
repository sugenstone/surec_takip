# Project domain foundation kararı

Tarih: 2026-09-23
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 17)

## Context

Reusable SaaS starter extraction (ADR 0005, adım 16) tamamlandı; ürün
repository'sinde proje-özel domain geliştirme kapısı açıldı. Projects, genel
SaaS foundation (auth, organizations, workspaces, tenant context, RBAC,
invitations, app shell) üzerindeki **ilk ürün-domain aggregate**'ıdır ve
gelecekteki Sections → Work Items/Processes hiyerarşisinin stabil ebeveyni
olacaktır. Bu adım YALNIZCA Project katmanını kurar.

Bu ADR'de "görev" (step 17 prompt), DATABASE_SCHEMA.md §5'teki kanonik
`projects` tablosundan farklı adlandırma seçti: `slug` (public_code yerine) ve
`status` (system_status yerine). Görev en güncel insan talimatı olarak
AGENTS.md §2 önceliğinde bağlayıcıdır; kanonik şema dokümanı bu task'ta
uygulamaya eşitlendi. Fark ve gerekçe: slug, organizations/workspaces ile
aynı üretim/normalize/tekillik geleneğini (citext + parent-scoped unique)
sürdürür; status, küçük V1 yaşam döngüsünü doğrudan adlandırır.

## Decision

1. **Sahiplik hiyerarşisi:** `organization (tenant root) → workspace →
project`. Bir project tam olarak TEK workspace'e aittir ve o workspace tam
   olarak TEK organization'a. Uygulama düzeyinde sahiplik asla yalnız
   `workspace_id` çıkarımıyla ifade edilmez; tenant tutarlılığı DB katmanında
   kısıtlanır.
2. **DB invariant (`007_projects`):** `FOREIGN KEY (tenant_id, workspace_id)
REFERENCES workspaces (tenant_id, id)` — tenant'ı A olan bir project
   satırının tenant'ı B olan workspace'i göstermesi **depolanamaz** (test
   edildi). Bu, workspace_memberships'in 004'teki deseniyle aynı composite-FK
   kapanışıdır.
3. **Route parent authority:** tek yetkili yol
   `/api/v1/organizations/{org}/workspaces/{ws}/projects[/{project}]`.
   Project id TEK BAŞINA hiçbir şey çözmez; yanlış workspace parent'ı, yanlış
   org parent'ı veya her ikisi birden — kullanıcı iki workspace'in de
   org'un da meşru üyesi olsa bile — uniform 404 döner (test edildi).
   Ham-ID'li alternatif erişim yolu YOKTUR.
4. **ProjectContext:** adım 10'un named-path struct mimarisinin devamı olarak
   `ProjectRoute {organization_id, workspace_id, project_id}` ile
   `ProjectContext` extractor'ı eklendi. Çözüm zinciri:
   auth (401) → üç UUID parse (malformed == unknown 404) →
   `workspaces::find_accessible` (iki membership + parent authority) →
   `projects::find_accessible` (id ∧ resolved workspace ∧ resolved tenant ∧
   deleted_at IS NULL tek sorguda). Context yalnızca request
   ELIGIBILITY çözer; mutasyonlar transaction içinde yeniden kanıtlar.
5. **Okuma politikası:** liste ve tekil okuma eligibility-based'tır; V1'de
   `projects:read` YOKTUR (mevcut ürün politikası workspaces ile tutarlı:
   görebilen üye okur). Liste resolved workspace'e scople'dır, deleted
   satırları içermez, active/completed/archived dahil edilir (UI filtreler),
   sıralama `created_at DESC, id` (deterministik, dokümante).
6. **Mutasyon permission'ları:** `projects:create`, `projects:update`,
   `projects:archive` (yalnızca gerçek enforcement noktası olan
   anahtarlar). Workspace resource olduğu için değerlendirme
   `authorize_workspace*` semantiğiyle yapılır: organization-scope grant
   tenant genelinde geçerli, workspace-scope grant yalnız kendi workspace'ine
   bağlı (workspace dışına çıkamaz — test edildi). Owner bootstrap listesi ve
   007 migration backfill'i mevcut Owner ROLLERİNE grant verir; hiçbir
   kullanıcıya Owner ROLÜ atanmaz (ADR 0008 kararının devamı; test edildi).
7. **Status yaşam döngüsü (bilinçli olarak küçük):** `active | completed |
archived`, DB CHECK + uygulama tarafı geçiş doğrulaması. İzinli geçişler:
   active→completed, active→archived, completed→active, completed→archived,
   archived→active + identity (aynı durum). `archived→completed` bilinçli
   olarak KAPALI (önce active'e dönmek gerekir). Soft delete (deleted_at)
   yaşam döngüsünden bağımsızdır; archived ≠ deleted.
8. **Archive semantiği:** `projects:update` tüm PATCH'lerde zorunludur;
   request `status: "archived"` SET EDİYORSA ek olarak `projects:archive`
   gerekir (veri modeli üzerinden dolaylı arşivleme kapalı — test edildi).
   Unarchive (archived→active) yalnızca `projects:update` ister; basitlik
   tercihidir ve `projects:archive`'ı ölü/erişilemez anahtar yapmaz.
9. **Slug semantics:** mevcut `slug_from_text`/`validate_slug` yeniden
   kullanıldı (yeni algoritma yok). `UNIQUE (workspace_id, slug)` HARD;
   workspace_id global tek (uuid PK) olduğundan per-workspace teklik
   per-tenant tekliği içerir; sorgular yine savunma derinliği olarak
   tenant_id ile filtreler. Case-insensitive teklik citext'ten gelir (test
   edildi); çakışma `VALIDATION_ERROR` + `fields.slug` ile stabil 422'ye
   map'lenir (ham PostgreSQL hatası sızmaz). Concurrent aynı-slug create
   tam olarak tek kazanan (test edildi).
10. **Transaction revalidation / TOCTOU:** create ve update kendi
    transaction'ında şu gerçekleri yeniden kanıtlar: aktif org membership,
    aktif ws membership, parent authority (workspace tenant'a ait ve
    silinmemiş), permission (`authorize_workspace_in_tx`), ve update için
    ayrıca `SELECT ... FOR UPDATE` ile satır kilidi + geçiş doğrulaması.
    Handler-öncesi kontrol yalnızca hızlı yoldur; membership revoke, grant
    revoke ve workspace silinmesi yarışları deterministik testlerle
    kapatıldı (0 yetkisiz satır yazılır).
11. **401/404/403 semantics:** 401 AUTH_REQUIRED (session yok) → 404
    RESOURCE_NOT_FOUND (erişilemez/yabancı/malformed/silinmiş/parent
    uyuşmazlığı — hepsi ayırt edilemez, fingerprint-parite testli) → 403
    PERMISSION_DENIED (eligible ama yetkisiz mutasyon). Foreign kaynak için
    asla 403 dönülmez.
12. **Gelecek Sections ilişkisi:** `projects.id` → `sections.project_id`
    FK'sı sonraki adımda ekleneğe hazır; project stabil ebeveyn
    aggregate'tır. Aynı composite-FK deseni (`(tenant_id, project_id)` hedef
    unique'i) sections migration'ında uygulanacaktır.

## Non-goals (bu adımda bilinçli olarak YOK)

- Sections, iç içe section ağacı, Work Items, Processes, process groups,
  dependencies, timers, assignments, teams, cards, TV, QR, dashboard,
  progress.
- DELETE/restore endpoint'i: `deleted_at` yalnızca şema altyapısıdır;
  ürün semantiği (kim silebilir, geri alınabilir mi, audit) tanımlanmadan
  endpoint açılmaz. Soft-delete satırlarının okuma yollarından düşürülmesi
  geçerlidir ve testlidir.
- `projects:read` permission'ı, cursor pagination, search/filter query
  parametreleri, revision/optimistic concurrency, audit kayıtları, realtime
  olayları — ilgili roadmap fazlarında.
- Starter repository'sine backport: YOK. Projects yalnızca ürün
  repository'sinde yaşar (ADR 0005 extraction boundary).

## Consequences

- Workspace-scoped yetkilendirme için `authorize_workspace_in_tx` RBAC
  servisine eklendi (mevcut `authorize_organization_in_tx` deseninin
  workspace karşılığı).
- `workspaces::find_accessible` generic executor imzasına genelleştirildi
  (transaction snapshot'ında yeniden çalışabilir); mevcut çağrılar etkilenmez.
- Yeni endpoint: workspace-seviye `effective-permissions` (UI görünürlüğü
  için; workspace-scope grant'ların UI'da görünmesini sağlar, yetki
  otoritesi backend'dir).
- Migration testleri 7-migration gerçekliğine güncellendi (revert → projects
  kalkar + öncekiler kalır; FK probe projects'e bağlanır).
