# Başlangıç incelemesi ve uygulama planı

Tarih: 2026-09-21

## İnceleme

AGENTS.md, master plan, database schema, API contract, UX/UI spec,
design system ve README bu sırayla tamamen incelendi. Başlangıçta yalnızca
bu yedi belge vardı; kaynak kod, migration, test, CI ve Git deposu yoktu.
Mevcut çalışma dizini repository köküdür; belgelerdeki `platform/` örneği
için ek bir iç dizin oluşturulmayacak.

## Öncelik kurallarıyla çözülen farklılıklar

- Master planın ilk sprint sırası ile AGENTS.md §36 ayrıntılı sırası farklı.
  First Agent Mission uygulanacak. Users/sessions, tenant context için
  gerekli temeldir; tam auth/RBAC kapsamı Faz 3 kabul kriterleriyle kapanır.
- Master plan §1.2 Redis'i Compose listesine alırken §32 ve README ihtiyaç
  oluşmadan eklenmemesini ister. Yerel altyapı adımında bu fark açıkça
  belgelenecek; Redis kullanılmadan çalışan bir servis bağımlılığı yapılmayacak.
- Şema §22 audit/outbox tablolarını 015'e bırakıyor; önceki fazlardaki rol,
  silme/restore ve zaman komutları bunlara ihtiyaç duyuyor. AGENTS.md audit
  ve transaction kuralları önceliklidir. İlk ilgili mutasyondan önce gereken
  temel tabloların migration sırası şema belgesinde güncellenecek. Audit UI,
  geçmiş sürümler ve realtime dağıtımı kendi fazlarında kalacak.
- Üyelik tablolarında `deleted_at` listelenmiyor; Faz 2 silinmiş üyeliklerin
  erişim vermediğini test etmeyi şart koşuyor. Üyelik migration'ından önce
  şema soft delete ve aktif üyelik kontrolünü açıkça tanımlayacak.
- API §9/10 `revision`, §17/39 `expected_revision` kullanıyor. İlk revision
  destekli endpointten önce istekler `expected_revision`, yanıtlar `revision`
  olarak sözleşmede birleştirilecek; sessiz eşzamanlı overwrite olmayacak.
- Şema locale sütunlarına ek olarak `settings.default_language` örneği veriyor.
  Dil çözümünde `users.locale → organizations.default_locale → tr-TR`
  kullanılacak; iki bağımsız dil doğruluk kaynağı oluşturulmayacak.
- README hedef ağacında DESIGN_SYSTEM.md eksikti; aynı değişiklikte eklendi.

Bunlar ilk monorepo adımını engellemez ve yeni ürün kararı gerektirmez.
Henüz veri veya yayımlanmış API olmadığı için geriye uyumluluk kırılması yoktur.

## İlgili fazlardan önce tamamlanacak sözleşme eksikleri

- Auth register/onboarding, invitation ve password-reset/verification token
  modelleri ayrıntılandırılmamış. Token hash/expiry/revocation ve endpoint
  payload'ları auth diliminde şema/OpenAPI/testlerle birlikte tanımlanmalı.
- Tenant içi ilişkileri koruyan composite FK/unique kuralları, üyelik durum
  değerleri ve üyelik geri yükleme semantiği migration'larda somutlaştırılmalı.
- Idempotency kaydı/saklama modeli, section/card duplicate payload'ları,
  delete/restore alt nesne davranışı ve yetki kapsamları ilgili dilimlerde
  netleştirilmeli; gelecek faz tabloları topluca oluşturulmamalı.
- Makine tarafından okunabilir OpenAPI, CI ve hiçbir otomatik test henüz yok.

## Küçük ve doğrulanabilir uygulama adımları

Her adımda Inspect → Plan → Implement → Verify → Review → Document uygulanır.

1. Monorepo: Git, private npm workspaces, ortak editör/ignore ayarları,
   lockfile ve gerçek durumu gösteren README. Workspace çözümlemesi ve temiz
   kurulum doğrulanır. Uygulama çalışıyor iddiası yapılmaz.
2. SvelteKit + TypeScript: frontend yapılandırması, tr-TR/en çeviri temeli,
   semantik tokenlar; typecheck/lint/unit/build doğrulaması. Auth app shell
   First Agent Mission adım 14'te geliştirilir.
3. Rust/Axum: Cargo workspace ve minimal server; format/clippy/test kontrolleri,
   gerçek endpoint varsa OpenAPI. Worker yalnızca gerçek göreviyle uygulanır.
4. PostgreSQL + SQLx migration tooling: ayrı test veritabanı, migration
   uygulama/doğrulama; domain tabloları kendi adımlarında.
5. Docker yerel ortamı: çalışan servisler ve tek komutla başlatma doğrulaması.
6. CI: format/lint/typecheck/unit/integration/migration/contract kontrolleri;
   Faz 1 ancak yerel başlatma ve CI koşulları doğrulanınca tamamlanır.
7. Users/sessions → organizations/memberships → workspaces/memberships →
   tenant middleware; minimum gerekli audit temeli ilgili mutasyonla birlikte.
8. Cross-tenant, guessed UUID, workspace ve silinmiş üyelik entegrasyon testleri.
   Geçmeden ilerlenmez.
9. Roles/permissions → minimal app shell → projects → recursive sections ve
   cycle prevention → cards ve soft delete/restore.
10. Faz 1–4 kabul kriterleri doğrulanır. Sonrasında Dynamic Properties açılır.

## İlk dilimin etki incelemesi

Dosyalar: kök manifest/lockfile, workspace manifestleri, ortak dosya ayarları,
canonical dizin yer tutucuları, README ve bu inceleme kaydı.
Yeni runtime bağımlılığı veya framework eklenmedi. JavaScript workspaces
mevcut npm ile yönetilir; Rust workspace Rust adımında kurulacak.
Schema/API/permission/realtime/audit/undo etkisi yok; veri mutasyonu veya
kullanıcı arayüzü yok. Uygulama testleri henüz çalıştırılabilir değildir.

README bakım değerlendirmesi: Geliştiricinin kurulumunu ve repository yapısını
değiştiriyor mu? Evet; README aynı değişiklikte güncellenecek ve komutları
çalıştırılarak doğrulanacak.

## Ortam incelemesi

Git 2.55.0, Node 24.21.0, npm 11.6.0, Cargo/rustc 1.92.0,
Docker CLI 29.8.0 ve Compose 5.5.1 sürüm komutları çalıştı.
Bu tespitler uygulama derleme veya Docker daemon doğrulaması değildir.
`psql` PATH üzerinde bulunamadı. `pnpm --version` kullanıcı dizinine erişimde
EPERM verdi; mevcut npm workspace yönetimi için yeterlidir.
Docker CLI kullanıcı config dosyasına erişim uyarısı verdi; yerel altyapı
adımında daemon ve container çalıştırma ayrıca doğrulanmalıdır.

## İlk adım doğrulama sonucu

- Git deposu `main` dalıyla oluşturuldu; commit veya uzak repository eklenmedi.
- `npm ci --ignore-scripts --offline --no-audit --no-fund --cache .npm-cache`
  temiz kurulumda başarılı; tekrar çalıştırılması lockfile hash'ini değiştirmedi.
- `npm ls --workspaces --depth=0` ve `npm pkg get name --workspaces`
  üç yerel workspace'i doğru çözümledi.
- Manifestler JSON olarak okundu; kök ve üç paketin private olduğu, runtime
  bağımlılığı eklenmediği doğrulandı.
- `git check-ignore` ile yerel env, node_modules, npm cache ve Rust target
  yollarının dışlandığı; `.env.example` ve lockfile'ın dışlanmadığı doğrulandı.
- README kurulum/development komutları bu ortamda çalıştırıldı.
- Rust format/clippy/test, TypeScript/lint/unit, migration/integration ve
  Playwright kontrolleri henüz kaynak kod ve yapılandırma olmadığından
  uygulanabilir değil; geçmiş veya başarılı sayılmadı.

First Agent Mission adım 1 tamamlandı. Faz 1 ve First Agent Mission bütünü
açıktır. Sonraki uygulama dilimi adım 2, SvelteKit + TypeScript temelidir.
