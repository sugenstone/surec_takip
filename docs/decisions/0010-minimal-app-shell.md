# Minimal app shell kararı

Tarih: 2026-09-23
Durum: Kabul edildi ve uygulandı (First Agent Mission adım 15)

## Context

Adım 7–14 generic SaaS foundation'ı (auth, tenancy, RBAC, invitations)
backend'de tamamladı. Adım 15, bu foundation'ı kullanıcının deneyimleyeceği
minimal authenticated shell'e dönüştürür. Ürün domain kavramı içermez;
Sugenstone SaaS Starter'a doğrudan dahil edilebilir olmalıdır.

## Kararlar

1. **Routing modeli — URL-as-authority:** organization/workspace kimliği
   URL'dedir: `/app/[organizationId]/[workspaceId]`. Deep-linking, refresh
   stability, browser back/forward, multi-tab ve cookie-sync sorunları
   çözülür. Cookie'ler (`organization`, `workspace`) yalnızca UX tercihi
   olarak kalır — `/app` root'unda default seçim için hint; authorization
   ASLA cookie'den gelmez.
2. **Layout hiyerarşisi:** `/app/+layout.server.ts` yalnızca auth guard'ı
   yapar (redirect yapmaz — nested route'lar kırılmasın). Org selection
   redirect'i `/app/+page.server.ts`'tedir. `/app/[orgId]/+layout.server.ts`
   workspaces + effective-permissions'ı backend'den çöker (session cookie
   forward edilir). `[wsId]/+layout.server.ts` workspace'i doğrular ve
   parent'ın AppShell'ini kullanır (ikinci shell render etmez).
3. **Stale-context recovery:** URL'deki orgId kullanıcıya ait değilse →
   `/app` redirect (yeniden seçim). URL'deki wsId geçersizse → org page
   redirect. Cookie'deki org silinmişse → fallback ilk erişilebilir org.
   Cross-tenant varlığı sızmaz (404 yerine sessiz redirect).
4. **Permission-aware UI:** Yeni minimal endpoint
   `GET /organizations/{org}/effective-permissions` — context-scoped,
   membership-scoped effective permission key'leri döner. Frontend action
   visibility için kullanır (örn. workspace-create form'u yalnızca
   `workspaces:create` varsa görünür). Backend 403 authoritative kalır;
   frontend hiding güvenlik değildir. Rol-adı kontrolü ASLA yapılmaz.
5. **Multi-tab:** URL-addressed context sayesinde Tab A Org A/Ws A1'de
   kalırken Tab B Org B/Ws B1'de kalabilir; cookie diğer tab'ı zorla
   değiştirmez (yalnızca yeni navigasyonda hint).
6. **SSR strategy:** hooks.server.ts `/auth/me` + org listesi çözer (mevcut
   devam). Org layout sunucu tarafında workspaces + permissions çöker
   (cookie forward ile). Login-flash/wrong-org-flash yok — SSR data hazır
   gelir. Client waterfall yok.
7. **Shell mimarisi:** `AppShell` component (header + switchers + account +
   nav + main). Decomposed; tek dev +layout.svelte yok. Semantic tokens;
   hardcoded renk yok. Mobile: hamburger nav (768px altı), thumb-friendly
   switcher'lar, yatay overflow yok (test edildi).
8. **Root `/` → `/app`:** authenticated root artık `/app`'e redirect eder.
   `/app` no-org empty state render eder veya org page'e redirect eder.
   `/` → login → `/app` zinciri redirect-loop içermez.
9. **Starter readiness:** Tüm shell kodu generic'tir — auth shell, tenant
   switcher, workspace switcher, theme, locale, account menu, responsive
   nav, empty states. Ürün-domain kavramı (Projects/Cards/Processes)
   ALINMAMIŞTIR.

## Consequences

- Eski `/` sayfası (welcome + org/ws create) shell'e taşındı; org.spec.ts
  ve workspace.spec.ts shell.spec.ts ile birleştirildi (onboarding testi
  full akışı kapsar).
- Root layout artık session-bar render etmez (AppShell devraldı); login
  page'nin minimal layout'u korunur.
- Effective-permissions endpoint OpenAPI'ye eklendi; frontend tipleri
  generated contracts'tan türetilir.
- Rate limiting, email delivery, workspace invitations, role CRUD hâlâ ertelenmiş.
