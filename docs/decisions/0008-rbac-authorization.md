# RBAC yetkilendirme modeli kararı

Tarih: 2026-09-22
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 13)

## Context

Generic SaaS foundation'da org/ws membership'ler eligibility (erişim hakkı)
verir; ayrıcalıklı işlemler için permission tabanlı RBAC gerekir (ADR
0005/0006/0007 sonrası). Henüz ürün-domain izni yok; model reusable altyapı
olarak kurulur.

## Kararlar

1. **İki kapı ilkesi:** `membership eligibility` (org ∧ ws membership, context
   extractor'ları) ile `permission authorization` (rbac modülü) ayrı
   kapılardır. Role ataması asla kendi başına membership/erişim yaratmaz;
   workspace permission'ı aktif workspace membership olmadan geçersizdir.
   RBAC, Adım 8–12 tenant izolasyonunu gevşetmez (tenant_isolation suite'i
   regression olarak kalır).
2. **Şema (canonical §4):** `permissions` (global katalog, UNIQUE key),
   `roles` (tenant-owned, is_system, gelecekte workspace-specific için
   nullable workspace_id), `role_permissions` (PK: role+permission+scope;
   scope ∈ organization|workspace — diğer canonical scope'lar kendi
   domainleriyle gelir), `membership_roles` (atamalar). Tenant tutarlılığı
   DB'de: `roles UNIQUE(tenant_id,id)` + `membership_roles (tenant_id,role_id)
→ roles(tenant_id,id)` composite FK; `(tenant_id,workspace_id) →
workspaces(tenant_id,id)`; `(tenant_id,user_id) →
organization_memberships(tenant_id,user_id)` (üye olmayana atama
   yazılamaz). Org-geniş atama tekliği partial unique index'le.
3. **İzin anahtarları:** stable machine key'ler (`workspaces:create`).
   Yeni anahtar yalnız gerçek enforcement noktasıyla eklenir. Authorization
   asla role adı/etiket karşılaştırması yapmaz; builtin role adları ('owner',
   'member') stabil tanımlayıcıdır, çeviriye bağımlı değildir.
4. **Kapsam semantiği (bilinçli en küçük model):** org-geniş atama
   (workspace_id NULL, rp.scope='organization') tüm tenant'a — workspace'ler
   dahil — geçerlidir; workspace-scoped atama yalnız kendi workspace'ine.
   role_permissions.scope, iznin yetki sınıfını bildirir; atamanın
   workspace_id'si nereye uygulandığını. team/self/assigned_projects
   temsil edilmez (domainleri gelince).
5. **Owner bootstrap:** org yaratma transaction'ı genişledi: org + creator
   membership + builtin 'owner'/'member' rolleri + owner'ın
   workspaces:create grant'i + creator'ın owner ataması TEK tx'te. Yöneticisiz
   org penceresi yok; owner permission-tabanlıdır (magic branch değil);
   gelecekteki yeni izinler owner'a otomatik akmaz — bilinçli
   grant/migration gerekir. Workspace yaratıcısına ekstra workspace rolü
   verilmez (canonical desteklemiyor).
6. **Backfill (pre-RBAC org'lar) — GÜVENLİ STRATEJİ:** migration, mevcut her
   org'a builtin rolleri + owner grant'ini ekler ama **hiçbir otomatik Owner
   ataması yapmaz**. "En erken aktif üyelik = Owner" deterministiktir ancak
   AUTHORITATIVE değildir: schema creator kaydı tutmaz ve import edilmiş
   veride sıradan bir üye ilk sırada olabilir — otomatik backfill yetki
   yükseltmesi olurdu (final review'da düzeltildi). Pre-RBAC org'lar
   **açık, güvenilir bootstrap** ister: `npm run user:grant-owner --
<organization_id> <email>` (user-admin CLI; yalnızca aktif org üyesine
   izin verir, idempotent; HTTP yüzeyi yok).
7. **Privilege resurrection (executable):** `deactivate_membership_with_assignments`
   membership'i soft-delete eder VE atamaları FİZİKİLEN siler (tek tx);
   `reactivate_membership` atamaları geri getirmez — eski ayrıcalıklar
   dirilemez, yalnızca açık yeni atama yetki verir (test: 8 adımlı döngü).
   Fiziksel silme bilinçli tercihtir: soft-delete'li grant'ler yanlışlıkla
   dirilme riski taşar. Ek savunma: authorization sorguları her zaman aktif
   membership join'ler — stale satır asla etkisiz değil etkisiz+inert'tir.
8. **403 vs 404:** eligibility başarısız → 404 RESOURCE_NOT_FOUND (varlık
   sızmaz); eligible ama yetkisiz → **403 PERMISSION_DENIED** (stable code,
   genel mesaj — role/permission içyüzü sızmaz). 401 yalnız auth eksikliği.
9. **TOCTOU:** yetki kontrolleri handler'da VE mutation transaction'ının
   içinde (`authorize_organization_in_tx`) tekrar yapılır; check ile commit
   arasındaki revoke pencereleri kapanır (test: permission TOCTOU +
   membership TOCTOU).
10. **Endpoint politikası (bilinçtel):** POST /organizations platform-auth
    (tenant yok); GET orgs/ws'ler eligibility (membership semantics korunur);
    **POST workspaces → workspaces:create @ org scope (403/404 politikası
    testli)**; yeni GET permissions/roles katalog okumaları eligibility.
    Role CRUD/atama API'leri bu adımda YOK (Step 14+/admin fazına ertelendi;
    atama yolları service+SQL ile test edildi).
11. **Context sınırı korunur:** context eligibility/identity taşır;
    permission'lar snapshot olarak context'e doldurulmaz (stale-snapshot
    riski) — rbac servisi her yetkili istekte DB'den çözer. Cache yok;
    gelecekte semantics değiştirmeden eklenebilir.

## Review ile eklenen bağlayıcı invariant'lar

- **Pre-grant önleme (DB):** `membership_roles (workspace_id, user_id) →
workspace_memberships(workspace_id, user_id)` composite FK — workspace
  membership satırı olmayan kullanıcıya ws-scoped atama DEPOLANAMAZ (NULL
  workspace_id'de FK atlanır). Dorman-privilege riski yapısal olarak kapatıldı.
- **Rol↔workspace tenant tutarlılığı (DB):** `roles (tenant_id, workspace_id)
→ workspaces(tenant_id, id)` composite FK — tenant A rolü tenant B
  workspace'ine bağlanamaz.
- **Workspace resurrection (servis):** `deactivate_workspace_membership_with_assignments`
  ws membership'i soft-delete eder VE o workspace'e ait ws-scoped atamaları
  fiziksel olarak siler; `reactivate_workspace_membership` onları geri
  getirmez (8 adımlı test). HTTP API yok — servis primitive + test.
- **authorize_workspace kendi ws-membership join'ini taşır:** org-wide grant
  olsa bile aktif workspace membership yoksa servis REDDEDER (context
  eligibility'ye güvenmek yerine savunma kendi sorgusunda).
- **Kapsam tutarlılık invariant'ı:** atamanın etkili kapsamı = rol kapsamı ∩
  atama kapsamından dar olan; anlamsız kombinasyonlar (ws-tied rol + org-wide
  atama; scope='workspace' grant + org-wide atama) authorization SQL'inde
  eşleşmez (inert) ve desteklenen yazma yollarında reddedilir. Gelecekteki
  atama API'leri bu invariant'ı service katmanında zorunlu kilar.

## Consequences

- Çoklu aktif rol → izin BİRLİŞİMİ (UNION); deny modeli yok (testli).
- membership_roles append+delete yaşam döngüsülüdür (updated_at/deleted_at
  yok — canonical); geçmiş atama kaydı audit fazının işi.
- Invitations (adım 14) kabul anında atama INSERT'i yapabilir; membership FK
  - teklik + write-path invariant'ları (aynı tx'te org membership doğrulama)
    bağlayıcıdır.
- Last-owner: hâlâ removal endpoint'i yok; invariant tanımı: owner-ataması
  kaldıran/removal yapan işlem, son aktif owner-atamasını bırakmayı aynı
  tx'te reddetmelidir (adım 14+/membership-management'ta uygulanacak).
