# Work Items domain foundation

Tarih: 2026-09-23
Durum: Uygulandı; local verification / review milestone (commit edilmedi)

## Model ve sınır

Work Item (TR: İşçilik), tam olarak bir organization → workspace → project →
section zincirine aittir. Recursive değildir; hiyerarşi yalnız Sections'a
aittir. `work_items` canonical adı STEP 19 talimatı ve ADR 0011/0012'nin Work
Items sınırıyla uyumludur. Eski conceptual `cards` taslağı bu uygulama değildir.
Sektör tipi, process, timer, atama, yüzde veya dinamik property eklenmedi.
Gelecekte Process bir Work Item'ın çocuğu olabilir; bu adım Process tablosu,
rotası veya ilerleme hesabı oluşturmaz.

## Persistence / migration 009

`20260923160000_work_items`: UUID id; tenant_id, workspace_id, project_id,
section_id; name, citext slug, integer position, status; created_at,
updated_at, deleted_at. Scope kolonları NOT NULL. Dört kolonlu FK
`(tenant_id, workspace_id, project_id, section_id)` mevcut
`sections(tenant_id, workspace_id, project_id, id)` unique hedefini kullanır.
Sections'ın project FK'si üzerinden zincir tamamen DB tarafından korunur.
Yanlış section veya tenant/workspace/project birleşimi depolanamaz.

Slug normalizasyonu mevcut `slug_from_text`/`validate_slug` ile aynıdır.
`UNIQUE(section_id, slug)` hard ve case-insensitive'dir. Archive/soft delete
slug'ı serbest bırakmaz; aynı slug başka section'da kullanılabilir.
Position create'de section içindeki tüm satırların max+1 değeriyle eklenir;
PATCH açık position kabul eder. Eşit pozisyonlar `(position, id)` ile sıralanır.
Sıralama index'i `(section_id, position, id)`; teklik zorlaması yoktur.
Integer taşması raw DB hatası yerine position validation üretir.

Down yalnız work_items tablosunu, bu üç iznin grant'lerini ve catalog
satırlarını kaldırır. CASCADE yoktur; bağımlı FK rollback'i engeller. Önceki
sekiz migration değiştirilmedi. Backfill mevcut system Owner rollerine grant
verir; membership_roles'a yazmaz, Owner kullanıcı tahmin etmez. Yeni org
bootstrap'i aynı üç izni açık listeden verir; Member yetki kazanmaz.

## API / yaşam döngüsü

Prefix: `/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items`.

- GET collection: `{data: WorkItemPublic[]}`, section-scope, `(position,id)`.
- POST: name ve optional slug; sunucu scope ve active durumunu belirler,
  `201 {data: WorkItemPublic}`.
- GET `/{work_item_id}`: bare WorkItemPublic (mevcut detail convention).
- PATCH `/{work_item_id}`: name, slug, position, status; `200 {data: ...}`.

Status: active, completed, archived. Completed yalnız manuel bir etikettir;
Process completion/progress değildir. Görünür active/completed durumları
birbirine ve archived'a geçebilir. Archive, Projects/Sections gibi PATCH
ile yapılır; `work_items:update` yanında `work_items:archive` gerektirir.
STEP 19'un açık gereği olarak archived Work Item list/get/update'ten düşer;
Projects/Sections'ın arşiv görünürlüğü değiştirilmez. Archive satırı silmez
ve deleted_at'ten ayrıdır. Tekrar PATCH ile geri açma/restore, trash ve Undo
bu milestone'da yoktur; veri fiziksel olarak korunur. Bu sınırlama UI'da
arşiv onayında anlatılır; geri alma vaadi verilmez.

Section move ertelendi; section_id veya başka scope kolonları mutation
DTO'sunda kabul edilmez. `deny_unknown_fields` scope injection ve create
status override denemelerini reddeder. Slug boş create'de isimden türetilir;
boş PATCH slug geçersizdir. Eksik PATCH alanları mevcut değerleri korur.

## Context / authorization

WorkItemSectionContext, ProjectContext + route section çözümüyle active
section ve arşivlenmemiş project gerektirir. WorkItemContext bunun üstüne
tam scope'lu visible item sorgusu ekler. Named path struct extraction
kullanılır; gerçek context daha derin bir test rotasında doğrulanır.
Cookie'ler yetki girdisi değildir; session user + route + DB memberships
ve permission key tek otoritedir.

Reads eligibility-based. Mutations `work_items:create/update/archive`
anahtarlarıyla mevcut workspace permission semantiğini kullanır; org-wide
grant tenant genelinde, workspace grant yalnız kendi workspace'inde geçerli.
Role adı request authorization için kullanılmaz.

Sıra: 401 authentication → 404 scope/eligibility → 403 permission → 422
alan/domain validation. JSON/type/unknown-field hataları mevcut ApiJson
convention'ıyla 400 VALIDATION_ERROR olur. Result<ApiJson<...>, ApiError>
body rejection'ını yakalar; temel mutation permission kontrolü body hatası
sunulmadan önce çalışır. Archive izni, parse edilmiş archived isteğinde
alan validation'dan önce transaction içinde doğrulanır. Foreign, malformed,
unknown, archived/deleted work item tek generic 404 sözleşmesindedir.
Raw SQL hatası ve hata içindeki değerler response'a aktarılmaz.

## Transaction / concurrency

Her write transaction'ı project satırını FOR UPDATE kilitler (Sections
serialization sırası), ardından direct section, org/ws ve aktif iki
membership'i tam scope ile tekrar çözüp FOR SHARE kilitler. Arşivli/silinmiş
project veya direct section altında write yoktur. Mevcut Sections davranışı
korunur: bir section'ın üst atasının arşivlenmesi alt section'ları otomatik
arşivlemez; bu foundation direct section durumunu esas alır.

RBAC helper mevcut workspace permission predicate'ini kullanıp geçerli
grant/role/assignment/membership destek satırlarını FOR SHARE tutar. Önceden
commit edilmiş revoke reddedilir; doğrulamadan sonraki revoke transaction
bitene kadar bekler. Update mevcut item'ı scope + visibility koşuluyla
FOR UPDATE çözer; current row eksikse permission'dan önce not-found.
Aynı project mutasyonları ve append pozisyonları serileşir. Slug teklik
çatışması stable field error olur.

Yeni revision sistemi eklenmedi (mevcut Projects/Sections modeli). Row lock
atomik partial update sağlar, ancak eski tarayıcı draft'larının optimistic
conflict tespiti henüz yoktur. Aynı alana son başarılı yazma üstün gelir;
versioned editing gelecekte ayrı bir değişiklik gerektirir.

## Frontend / recovery

Mevcut section ağacına link; section-context SSR liste, empty/error/retry,
create; UUID deep-link detail ve edit/archive. Yetkisiz kontroller SSR
permissions ile gizlenir; backend 403 bağımsızdır. Form draft'ı başarısız
save'de korunur. Arşiv sonrası listeye dönülür; kalıcı bağlantı artık 404.
TR/en dictionary ve semantic tokenlar; label, keyboard focus, narrow layout,
dark mode aynı foundation'ı kullanır. Realtime/audit/outbox yeni sistemleri
kapsam dışıdır. Başarılı mutasyon sonrası mevcut invalidateAll convention'ı
kullanılır. Listede cursor pagination yoktur; section-sized foundation
listesi, mevcut Sections yaklaşımıyla tutarlıdır; büyük listeler gelecekte
sayfalanmalıdır.

## Verification scope

Gerçek PostgreSQL adversarial suite: CRUD, strong attacker nesting,
composite FK, 401/404/403/422 sırası, grant/revoke, workspace scope,
üyelik/permission/assignment/parent TOCTOU, archive/soft delete slug,
ordering, hostile cookies, gerçek deeper context, concurrent duplicate,
permission backfill ve migration rollback safety.

Browser: owner hiyerarşi ve create, URL/reload, edit ve korunmuş validation
draft, TR/en 360px/dark render, archive, eligible Member UI hiding + backend 403. E2E yalnız runner-owned ephemeral Compose DB'de role assignment'ı
fixture olarak değiştirir; production role API eklenmez. Bir ek seed user,
paralel spec'lerin onboarding varsayımlarını izole tutar. Test sayıları ve
exit code'ları milestone implementation raporunda kaydedilir.
