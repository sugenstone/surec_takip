# Organizations + organization memberships kararı

Tarih: 2026-09-22
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 8)

## Context

Users + secure sessions (ADR 0004) tamamlandıktan sonra sıradaki adım tenant
boundary'sidir: organizations ve organization memberships. Workspaces, full
tenant middleware, RBAC, invitations ve member-management endpoint'leri bu
milestone'un dışındadır.

## Decision

1. **organizations** canonical şemanın tamamıyla oluşturuldu (id, name,
   slug citext UNIQUE, status, default_timezone='UTC', default_locale,
   default_currency, settings jsonb, timestamp'ler, deleted_at).
   `organizations.id` tenant kimliğidir; tabloda tenant_id yok.
2. **organization_memberships** canonical kolonlarla (id, tenant_id, user_id,
   status, joined_at, created_at, updated_at, deleted_at) ve
   `UNIQUE (tenant_id, user_id)` ile oluşturuldu; user_id index'i eklendi.
3. **Hard UNIQUE semantiği:** unique kısıt kısmi (partial) değil,
   soft-deleted kayıtları da kapsar. Bir (tenant, user) çifti için en fazla
   tek satır var olur; yeniden katılma yeni satır değil mevcut satırın
   reactivate edilmesidir. Duplicate active membership concurrency altında
   DB seviyesinde imkânsızdır (entegrasyon testleri: ardışık + eşzamanlı
   deneme tek kazanan).
4. **Görünürlük kuralı** (tüm okuma yollarında ortak): membership
   `status='active' AND deleted_at IS NULL` VE organization
   `deleted_at IS NULL`. Silinen üyelik/organizasyon listede, `/auth/me`'de
   ve tekil getirmede görünmez.
5. **Slug:** açıkça verilen veya isimden türetilen her slug aynı
   normalizasyon hattından geçer (küçük harf, Türkçe transliterasyon,
   `[a-z0-9-]`, 1–64). Kullanılamaz slug 422 alan hatası; alınmış slug
   (citext, case-insensitive) 422 `details.fields.slug=["Already taken"]`.
   Slug public identifier'dır, asla authorization sınırı değil; authorization
   immutable UUID üzerinden. DB'de name/slug uzunluk CHECK'leri var.
6. **Atomik yaratma:** organization + creator membership tek PostgreSQL
   transaction'ında; membership başarısız olursa organizasyon da yok olur
   (test: var olmayan creator ile yaratma → organizasyon yok).
7. **Enumarasyon karşıtı hata semantiği:** üye olunmayan organizasyon,
   mevcut olmayan UUID, malformed ID, silinmiş üyelik, silinmiş organizasyon
   — hepsi aynı 404 `RESOURCE_NOT_FOUND` gövdesini döndürür.
8. **/auth/me** artık gerçek üyelik verisini döndürür (id/name/slug;
   `role_summary` RBAC fazına kadar boş dizi). Yalnız kullanıcının aktif
   üyeliklerinin gördüğü organizasyonlar.

## Extraction Boundary

Organizations/memberships generic SaaS foundation kapsamındadır (ADR 0005
Included: Multi-tenancy). Ürüne özgü kavram girmedi; slug transliterasyon
haritası i18n farkındalıklı ve domain-bağımsızdır.

## Last-owner ve RBAC sınırı

- Bu milestone'da üyelik çıkarma endpoint'i yok; last-owner enforcement
  uygulanmadı. Gelecekteki RBAC fazı owner rolünü `membership_roles`
  üzerinden bağlayacak; **geçiş dönemi convention'ı:** bir organizasyonun
  en erken aktif üyeliği (created_at, id sırasıyla) bootstrap owner olarak
  deterministik şekilde identifiabledır — backfill bunu kullanır. Membership
  tablosuna owner/role kolonu icat edilmedi (canonical şemaya uyum).
- Hard-coded `if role == "admin"` tarzı yetkilendirme yok; tek güvenlik
  sınırı üyelik kontrolüdür ve tek yerde (`find_for_member` +
  `visible_for_user`) uygulanır — adım 10'daki tenant middleware'e
  taşınmaya hazır.

## Future Usage (invitations)

Invitation kabul akışı membership INSERT'i doğrudan kullanabilir; hard
UNIQUE sayesinde davetin iki kez kabulü duplicate satır üretmez. Yeniden
katılma reactivate semantiği davet akışıyla uyumludur.

## Reactivate + membership_roles invariant (zorunlu, ileri fazlarda)

Canonical şemada `membership_roles` (tenant_id, user_id, role_id,
workspace_id) membership satırına FK ile değil doğrudan tenant+user üzerinden
bağlanır. Bu yüzden membership soft-delete edildiğinde mevcut role satırları
DB tarafından temizlenmez; ileride membership reactivate edilirse **eski
role atamaları kimse yeniden vermeden otomatik yeniden geçerli hale gelir.**
Bu sessiz privilege resurrection riskidir. İlgili fazlarda aşağıdaki
invariant'lar zorunludur:

1. Membership removal (soft delete) işlemi, aynı transaction içinde ilgili
   `membership_roles` satırlarını da geçersiz kılmalı/silmelidir.
2. Reactivate (yeniden katılma) varsayılan olarak **rolesuz** olmalıdır;
   roller yalnızca açıkça yeniden atanarak geri gelir.
3. Reactivate kodu, kalan (dangling) role satırları varsa işlemi reddetmeli
   veya temizlemelidir; asla sessizce devralmamalıdır.
4. Bu invariant'ın ihlali security regression olarak değerlendirilir ve
   cross-tenant testlerine membership_roles bağlandığında test edilmelidir.

## Consequences

- `/auth/me` ve organizasyon listesi deterministik sıradadır
  (created_at, id).
- Üyelik geçmişi (kim ne zaman ayrıldı) bu modelde satır üzerinde
  korunur; ayrıntılı üyelik geçmişi audit fazının (`audit_events`) işidir.
- Soft-deleted organizasyonun slug'ı UNIQUE kısıtta yer kapar (kasıtlı:
  slug geri dönüşümden sonra tekrar kullanılabilir olmalı mı ürün kararı
  later; şimdilik slug rezerve kalır).
