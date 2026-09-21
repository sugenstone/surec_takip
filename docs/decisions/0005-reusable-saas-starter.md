# Reusable SaaS Starter kararı

Tarih: 2026-09-22
Durum: Kabul edildi (planlama; extraction henüz yapılmadı)

## Context

Bu repository'de genel amaçlı bir SaaS altyapısı inşa ediliyor: users,
secure sessions, authentication, organizations, memberships, workspaces,
RBAC, tenant isolation ve mühendislik kalite kapıları. Bu foundation
Süreç Takip ürününden bağımsız olarak tasarlandı ve gelecekte başka
Sugenstone ürünlerinde de ihtiyaç duyulacak. Aynı foundation'ı her yeni
projede sıfırdan yeniden geliştirmemek için, doğrulanmış generic
foundation'dan reusable bir **"Sugenstone SaaS Starter"** çıkarmak proje
planının resmi bir milestone'u olur.

## Decision

1. Generic SaaS/security foundation tamamlanana kadar mevcut repository'de
   normal geliştirme sırası izlenir: Users/Sessions → Organizations →
   Workspaces → RBAC/Permissions → Invitations → Tenant Context → Tenant
   Isolation → Cross-tenant security tests.
2. Foundation tamamlandıktan ve hosted CI yeşil olduktan sonra, ancak
   Projects/Sections/Cards gibi ürün domain geliştirmesine başlanmadan
   ÖNCE **Reusable SaaS Starter Extraction** milestone'u uygulanır.
3. Extraction ayrı bir repository/templatesi üretir; mevcut surec_takip
   repository'si, geçmişi ve geliştirme akışı bozulmaz.
4. Starter bağımsız geliştirilebilir; semantic versioning (v1.0.0,
   v1.1.0, …) kullanması planlanır. Şu anda versioning implementasyonu
   yapılmaz.

Bağlayıcı koruma kuralı (AGENTS.md §43 ve master plan Phase 3.5'te de
yer alır):

> Project-specific domain development must not begin until the reusable
> starter extraction gate has been evaluated after completion of the
> generic multi-tenant/security foundation.

Bu kural generic foundation geliştirmesini engellemez; tam tersine
foundation bu kuralın ön koşuludur.

## Extraction Boundary

**Sınır ilkesi:** Generic altyapı katmanında ürün domain kavramı, ismi,
tablosu, endpoint'i veya varsayımı bulunmaz. Ürün domain'i
(surec_takip) yalnızca generic foundation'ın ÜZERİNDEki katmanda yaşar.
Extraction'dan önce her generic faz bu sınırı korumakla yükümlüdür;
sınırı ihlal eden sızıntı extraction zamanında tespit ve ayrıştırılır
(Phase 3.5.2 madde 1–3).

## Included Components

Starter'ın hedef kapsamı (mümkün olduğunca domain-independent):

- **Frontend:** SvelteKit, TypeScript, Tailwind, shared UI/design system
  foundation, semantic design tokens, light/dark/system tema, responsive
  application shell, accessibility foundation.
- **Localization:** i18n mimarisi, tr-TR + en, locale-aware formatting,
  tenant terminology foundation, hard-coded kullanıcı metni yasağı.
- **Backend:** Rust, Axum, structured errors, request ID'ler,
  tracing/logging, health/readiness endpoint'leri, configuration/environment
  sistemi.
- **Database:** PostgreSQL, SQLx, migration altyapısı, migration
  verification, reversible migration kuralları, UUIDv7 ve timestamptz/UTC
  konvansiyonları.
- **API:** REST `/api/v1` foundation, OpenAPI, üretilen TypeScript
  contract'ları, contract drift kontrolü, stable error code'lar.
- **Authentication:** users, password hashing, secure sessions, login,
  logout, current user/session, expiration, revocation, secure cookie
  davranışı.
- **Multi-tenancy:** organizations, memberships, workspaces, tenant
  context, tenant isolation, cross-tenant security testleri.
- **Authorization:** roles, permissions, RBAC, permission scope'ları,
  backend authorization foundation.
- **User lifecycle:** invitations, membership lifecycle foundation.
- **Engineering:** Docker, Compose, GitHub Actions CI, format/lint/type
  check, Rust fmt/clippy, unit/integration/E2E altyapısı, local quality
  gate'ler, pre-push verification, secret/environment kuralları.
- **AI-assisted development:** AGENTS.md (incremental verification ve
  operating kuralları dahil), README maintenance rules, canonical
  dokümantasyon yapısı, architecture decision record'lar.

## Excluded Domain Components

Starter'a taşınmayan, surec_takip ürün domain'inde kalanlar:

- Projects domain implementation
- Recursive Sections
- Cards domain
- Dynamic Properties domain
- Processes ve Process Steps
- Workflow dependencies
- Task assignments
- Time Sessions domain logic
- Employee "My Tasks"
- Active Work
- TV Operations
- Process/card templates
- Workflow-specific reporting

## Extraction Gate

Extraction yalnızca şu koşullar sağlandığında başlayabilir (Phase 3.5.1):

```text
users + secure sessions implement edildi ve testleri geçti
organizations + organization memberships
workspaces + workspace memberships
invitations
roles + permissions + RBAC
tenant context middleware
tenant isolation
cross-tenant security testleri yeşil
foundation commit'inde hosted CI yeşil
```

Extraction zamanındaki 15 maddelik agent yükümlülüğü (domain bağımsızlığı
incelemesi, sızıntı tespiti, kod ayrıştırma, kimlik değişkenleri,
starter README/AGENTS, PROJECT_BRIEF.md şablonu, yeni proje başlatma
prosedürü, tüm generic testler, Docker clean-start, migration
doğrulaması, auth + tenant isolation güvenlik testleri, starter repo'da
CI yeşilliği, secret/ürün-verisi temizliği, GitHub Template Repository
hazırlığı) master plan Phase 3.5.2'de bağlayıcı olarak tanımlıdır.

## Future Usage

```text
Sugenstone SaaS Starter
→ yeni repository template'ten oluşturulur
→ proje kimliği/environment yapılandırılır
→ PROJECT_BRIEF.md tamamlanır
→ coding agent AGENTS.md + PROJECT_BRIEF.md okur
→ agent proje domain'ini planlar
→ domain implementasyonu başlar
```

PROJECT_BRIEF.md alanları (örnek): Product, Purpose, Primary Users, Main
Domain Entities, Initial Language, Additional Languages, Branding,
Special Requirements.

## Consequences

- **Olumlu:** Yeni ürünler foundation'ı sıfırdan yazmaz; doğrulanmış
  güvenlik/tenancy/kalite kapısıyla başlar. Extraction gate, generic
  katmana ürün sızıntısını disiplinli tutar.
- **Maliyet:** Domain geliştirmesi başlamadan önce ek bir milestone
  (inceleme + ayrıştırma + doğrulama) ödenir; bu bilinçli seçimdir.
- **Sürdürülebilirlik:** Starter'daki ileriki değişiklikler türeyen
  ürünlere otomatik uygulanmaz; starter yeni projeler için başlangıç
  noktasıdır. Ürün kendi repository'sinde bağımsız devam eder.
- **Şu an için:** Bu karar yalnızca planlama ve dokümantasyon değişikliği
  getirir; kod, mimari, bağımlılık ve mevcut Users/Sessions çalışması
  etkilenmez.
