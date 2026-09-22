# User invitations kararı

Tarih: 2026-09-22
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 14)

## Context

Organizasyonlara üye ekleme, ADR 0006'dan beri SQL/CLI bootstrap'e bağlıydı.
Adım 14, privilege'lı üye → token → kimlik doğrulamalı kabul → membership +
güvenli rol akışını uygular. Generic SaaS altyapısıdır; ürün domain kavramı
içermez.

## Kararlar

1. **Token modeli (session ile aynı ilke):** 32 bayt CSPRNG → base64url
   (43 karakter). DB'de YALNIZCA SHA-256 digest saklanır (`token_hash`
   UNIQUE). Raw token sadece yaratma yanıtında bir kez döner (email altyapısı
   yok); list endpoint'lerinde asla. Token loglanmaz; request logları zaten
   yalnız route şablonu taşır. Acceptance: `POST /api/v1/invitations/accept`
   body'sinde — URL'e asla konmaz (log/browser sızıntısı).
2. **State machine (kolon yok, timestamp'ten türetilir):** pending =
   `accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now()`;
   accepted/revoked terminal. Öncelik: accepted > revoked > expired > pending.
   Tek kullanımlık: conditional UPDATE `accepted_at IS NULL` guard'ı.
3. **Email normalization:** citext kolon + `lower(email::text) = lower($2)`
   sorguları — users.email ile birebir aynı semantik; ikinci bir
   normalizasyon sistemi yok.
4. **Acceptance identity proof:** authenticated user AND citext-eşit account
   email. Email verification altyapısı olmadığı için `email_verified_at`
   gerektirilmez — bu bilinçcelimli bir V1 sınırıdır:
   - **Mevcut güven modeli:** kullanıcı hesabı YALNIZCA trusted `user-admin`
     CLI ile oluşturulabilir (public signup HTTP endpoint'i yok; email
     değiştirme endpoint'i yok). Bu yüzden email-equality, trusted
     provisioning'ten gelen bir ownership varsayımı taşır ve V1'de
     yeterlidir.
   - **Durable invariant (future signup):** public self-service hesap
     açılışı geldiğinde, invitation acceptance `email_verified_at IS NOT
NULL` gerektirmek ZORUNDADIR — doğrulanmamış arbitrary-email hesapları
     kabul edemez. Bu invariant ihlal edilirse davetiye sistemi açık bir
     hesap-claim saldırı vektörüne dönüşür.
     OrganizationContext gerektirilmez (aday henüz üye değil) — bu ayrı bir
     güvenlik sınırıdır (token+identity), context zayıflatılmaz.
5. **Inviter authorization:** yeni `members:invite` permission key'si
   (org scope). 401 → 404 (context) → 403 (permission) → 201. Migration 006
   hem mevcut Owner'lara hem de yeni-org bootstrap'ine açıkça grant eder
   (Owner gelecek izinleri otomatik almaz — explicit grant listesi).
   Member rolü invite izni ALMAZ.
6. **Role intent — V1'de YOKTUR:** davetiye YALNIZCA tenant'ın builtin
   `member` rolünü atar (stabil kimlik: tenant + is_system + 'member').
   İstemci role_id belirleyemez; Owner davetiyesi YOKTUR. "Invite edebilme"
   ≠ "privileged rol atayabilme" — role-assignment permission'ı gelecek
   membership-management fazına ayrılmıştır. Privilege escalation
   surface'i sıfırdır.
7. **Duplicate/re-invite:** partial UNIQUE `(tenant_id, email) WHERE pending`
   — aynı (tenant, email) için tek canlı token. Re-invite eski pending'i
   revoke eder + yeni token üretir (replace); eski token geçersiz.
   Concurrent duplicate invite → tek canlı satır (index).
8. **Expiration:** `expires_in_hours` opsiyonel (default 72, max 720; env
   değil — request field, merkezi sabitler). Süresi dolmuş → generic red.
9. **Acceptance transaction (atomik):** FOR UPDATE lock → conditional
   consumption (accepted_at IS NULL guard) → org alive check → membership
   create-or-reactivate → (yalnızca reactivation ise dangling atama
   temizliği) → builtin member rolü ata → commit. Herhangi bir adım
   başarısız → tam rollback; kısmi state yok.
10. **Reactivation (resurrection önleme):** membership 'deleted' durumundan
    reaktivasyonda YALNIZCA davetiyenin member rolü atanır; eski
    assignment'lar deactivation'da zaten fiziksel silinmişti — acceptance
    ek savunma olarak reactivation yolunda dangling satırları temizler.
    **Already-active member:** mevcut assignment'lara DOKUNMAZ (downgrade
    yok), member rolünü idempotent ekler (ON CONFLICT DO NOTHING).
11. **Workspace membership:** davetiye yalnızca ORG membership verir;
    workspace davetleri ertelenmiştir (org ≠ ws eligibility korunur).
12. **Inviter lifecycle (V1: immutable issued capability):** davetiye,
    yaratıldığı anda yetkili inviter tarafından üretilmiş bir capability'dir;
    acceptance inviter'ın halen üye/yetkili olup olmadığını yeniden
    doğrulamaz. Risk sınırlıdır (V1'de yalnızca member rolü verilebilir);
    privileged role intent geldiğinde yeniden değerlendirilir.
13. **Concurrency:** FOR UPDATE + conditional consumption → concurrent
    aynı-token acceptance'ı tam olarak bir başarılı; test deterministik
    (tokio::join).
14. **Enumeration resistance:** tüm acceptance başarısızlıkları (unknown/
    tampered/expired/revoked/consumed/wrong-email/deleted-org) TEK generic
    `422 INVITATION_INVALID` döner — durum ayrımı dışarıdan gözlenemez.
15. **Down migration:** invitations tablosu + members:invite permission +
    owner grant'leri (child→parent sırasıyla, CASCADE yok).

## Consequences

- Rate limiting hâlâ yok (davet yaratma/kabul abuse-sensitive; token
  entropisi + generic errors mevcut koruma).
- Raw token'ın yaratma yanıtında dönmesi MEVCUT V1 teslim handoff'udur (tek
  seferlik; list/revoke/error'da asla görünmez). Bu, "development-only"
  DEĞİLDİR — kod bu davranışı her ortamda uygular. Email/outbox delivery
  geldiğinde yaratma yanıtı token'ı KALDIRMAK ve teslimi email kanalına
  taşımak zorundadır; bu bir breaking API change olarak planlanmalıdır.
  Global `Cache-Control: no-store` middleware'i secret-bearing yanıtların
  cache'lenmesini önler.
- Audit events (invitation.created/accepted/revoked) outbox fazında.
- Frontend: bu milestone'da UI eklenmedi; backend authoritative.
