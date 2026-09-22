# Workspaces + workspace memberships kararı

Tarih: 2026-09-22
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 9)

## Context

Organizations (ADR 0006) sonrası tenant hiyerarşisinin ikinci katmanı:
workspace'ler bir organization'ın içindeki çalışma alanlarıdır ve tenant
DEĞİLDİRLER. Full tenant middleware (adım 10), RBAC ve invitations bu
milestone'un dışındadır.

## Decision

1. **workspaces** canonical kolonlarla oluşturuldu (id, tenant_id FK→
   organizations, name, slug citext, settings jsonb, timestamp'ler,
   deleted_at). Canonical modelde status kolonu yoktur; silinme yalnız
   `deleted_at` ile ifade edilir. `UNIQUE (tenant_id, slug)` — slug
   uniqueness **organization başına**dır (iki org aynı slug'ı
   kullanabilir); case-insensitive (citext). name/slug uzunluk CHECK'leri
   organizations ile aynı.
2. **workspace_memberships** canonical kolonlar + `deleted_at` ile;
   `UNIQUE (workspace_id, user_id)` **hard** (partial değil): ADR 0006
   membership semantiğinin birebir workspace karşılığı — bir
   (workspace, user) çifti için en fazla tek satır, rejoin = satır
   reactivate, duplicate active membership concurrency altında imkânsız.
3. **Composite FK ile cross-tenant imkânsızlığı:**
   `workspaces UNIQUE (tenant_id, id)` hedefine
   `FOREIGN KEY (tenant_id, workspace_id)` — workspace_memberships satırı,
   iddia ettiği tenant ile workspace'in gerçek tenant'ı uyuşmadıkça
   **depolanamaz**. Bu, cross-tenant membership saldırısını trigger'sız,
   saf DB kısıtıyla kapatır (test edildi).
4. **Erişim modeli (bağlayıcı):** bir workspace'in görünür/erişilebilir
   olması için **hem** geçerli organization membership **hem** geçerli
   workspace membership gereklidir; organization membership tek başına
   workspace erişimi vermez. Okuma yolları tek `ACCESS_FILTER` join'iyle
   (org membership ∧ ws membership ∧ satırlar silinmemiş) çalışır. Bu:
   - stale workspace membership'in, org membership silindiğinde ayrıcalık
     sağlamasını engeller (test edildi),
   - organization silindiğinde workspace erişimini keser (test edildi),
   - adım 10'daki tenant context middleware'e doğrudan taşınabilir tek
     sınırdır.
5. **Parent-child path authority:** `GET /organizations/{org}/workspaces/{id}`
   sorgusu `w.id = $id AND w.tenant_id = $org` ile çalışır; kullanıcı her iki
   org'a üye olsa bile yanlış parent-child kombinasyonu 404 döner. Bu pattern
   ilerideki nested resource'ların (project → section → card) şablonudur.
6. **Yaratma:** org membership transaction içinde yeniden doğrulanır;
   workspace + creator workspace membership tek tx'te commit olur — orphan
   workspace yok (test: var olmayan creator → rollback). Creator'ın
   otomatik workspace üyeliği canonical dokümanda açık yazmasa da pratik
   zorunluluktur: member-management endpoint'i olmadan creator üyeliği
   olmazsa workspace erişilemez kalırdı. Karar açıkça raporlanmıştır.
7. **Slug:** organizations ile aynı normalizasyon hattı (Türkçe
   transliterasyon, `[a-z0-9-]`, 1–64); duplicate slug yalnız aynı org
   içinde 422 "Already taken".
8. **/auth/me değişmedi:** canonical contract yalnız organization
   summaries öngörür; workspace'ler seçili organization'a göre ayrı list
   endpoint'inden alınır (N+1/tenant-ağacı şişirmesi yok).

## Write-path ve read-path invariant'ları (bağlayıcı)

1. **Row existence alone never grants access:** bir workspace_memberships
   satırının varlığı — hatta `status='active'` olması — tek başına erişim
   anlamına gelmez. Erişim yalnız read-path'in org-membership doğrulaması
   yapan ACCESS_FILTER join'inden geçer. Depoda org-üyeliksiz "honest
   tenant" satırı bulunabilir (FK bunu engellemez); erişim vermez.
2. **Every write path verifies organization membership:** workspace
   membership oluşturan veya reactivate eden her yazma yolu
   (`create_with_membership` bugün; gelecekte member-management ve
   invitation acceptance) aktif organization membership'i **aynı
   transaction içinde** doğrulamak zorundadır.
3. **Step 10 source of truth:** tenant context middleware'e taşınacak
   korumalı invariant'ların kaynağı şunlardır — `workspaces::ACCESS_FILTER`
   (+ `visible_for_user`, `find_accessible`) ve
   `organizations::VISIBLE_MEMBERSHIP_FILTER` (+ `find_for_member`,
   `visible_for_user`). Middleware bu fonksiyonların/filtrelerin
   anlamlarını gevşetmemeli; handler'lara kopyalanmış alternatif erişim
   sorgusu eklenmemelidir.

## Enumeration semantiği

Foreign org, foreign workspace, mismatched parent-child, rastgele UUID,
malformed UUID, silinmiş membership/workspace/organization → hepsi aynı 404
`RESOURCE_NOT_FOUND` gövdesi. 401 yalnız authentication eksikliğinde.

## Reactivate + gelecekteki roller (ADR 0006 invariant'ının genişlemesi)

ADR 0006'daki "membership soft-delete/reactivate eski membership_roles
atamalarını sessizce diriltemez" invariant'ı aynen workspace seviyesini de
kapsar: gelecekteki workspace-scoped role/grant yapıları için de (a)
removal aynı tx'te ilgili grants'i geçersiz kılar, (b) reactivate
varsayılan olarak privilegesızdır, (c) dangling grant satırları
reddedilir/temizlenir. Bu invariant'ın workspace testleri RBAC fazında
membership_roles workspace-scope bağlanırken yazılacaktır.

## Frontend context

Organization → workspace seçimi presentation-only cookie'lerledir;
workspace cookie'si SSR'da **sunucunun döndürdüğü permitted liste** (seçili
org'un workspace list endpoint'i) ile doğrulanır — foreign/stale değer org
değişiminde asla taşınmaz (org switch cookie'yi temizler + yeniden çözer).
Backend her istekte üyelikleri yeniden doğrular; frontend seçimi güvenlik
değildir.

## Consequences

- Workspace listesi yalnız kullanıcının iki üyeliğe de sahip olduğu
  workspace'leri döndürür; "org üyesi ama workspace üyesi değil" görünmez
  (test matrisi: A1 member / A2 non-member / B1 foreign / Shared).
- workspace_memberships.tenant_id sorgularda doğrudan tenant scoping
  sağlar; composite FK sayesinde workspace_id ile her zaman tutarlıdır.
- Slug rezervasyonu soft-deleted workspace'lerde de org bazında geçerlidir
  (bilinçli, organizations ile paralel).
