# Faz 1: platform foundation

## Kapsam ve uygulama planı

First Agent Mission 2–6: SvelteKit/TypeScript, i18n/tokens, Axum bootstrap,
PostgreSQL migration tooling, Docker ve CI. Domain ekranı, auth/session veya
tenant verisi bu değişiklikte uygulanmaz. Tenant authorization varmış gibi
davranan endpoint yoktur; yalnızca açık health/readiness probe'ları vardır.

Dosya kapsamı: apps/web, packages/ui, packages/contracts, apps/server,
migrations, scripts, docker, compose.yml, .github/workflows, README ve
ilgili canonical sözleşmeler. Şema etkisi PostgreSQL citext extension temeli;
ürün tabloları sonraki fazlarda. API etkisi /api/v1/health ve /api/v1/ready.
Permission, realtime, audit ve undo bakımından bu adımda domain mutasyonu yoktur.

## Kararlar

- Mevcut npm workspace yapısı korunur. SvelteKit adapter-node kullanılır.
  Svelte 5, TypeScript, Tailwind 4 ve ortak semantic CSS tokenları temel alınır.
  İhtiyaç duyulan etkileşimli bileşen olmadığından Bits UI/Lucide henüz eklenmez.
- Dil çözümü user → organization → tr-TR; en ikinci dil. Henüz user/organization
  bulunmadığından yalnızca doğrulanan presentation cookie'si kullanılır.
  Cookie tenant/auth kaynağı değildir. SSR dil durumu request-local kalır.
- Tema light varsayılanıdır; dark/system cookie tercihi SSR'da uygulanır.
  Yerleşim ve hata sayfası çevrilebilir. Dashboard veya ürün navigasyonu yoktur.
- Rust/Axum modular monolith; SQLx pool, sınırlı timeout, UUIDv7 request id,
  yapılandırılmış JSON log ve güvenli hata envelope'u. DB URL, request header,
  ham path/query veya SQL değerleri loglanmaz. Readiness gerçek SELECT 1 kullanır.
- OpenAPI Rust'tan üretilir, TypeScript tipleri OpenAPI'den türetilir; drift
  kontrolü aynı komutların CI'da tekrar ürettiği içeriği karşılaştırır.
- SQLx 0.8.6 Rust 1.92 uyumlu temel olarak seçildi; yeni major'a gereksiz geçiş
  yapılmadı. Cargo/npm lockfile'ları gerçek çözümlemeyi sabitler.
- Redis, object storage ve işlevsiz worker eklenmez; ilk gerçek ihtiyaçta
  ilgili fazla birlikte gelirler. Docker bu aşamadaki çalışan servisleri kapsar.

## Canonical düzeltmeler

Audit/outbox persistence ilk gerekli mutasyondan önce gelir; dispatch/history UI
kendi fazlarındadır. Üyeliklerde deleted_at ve aktif/silinmemiş kontrolü açıklandı.
Mevcut üyelik uniqueness kuralı korundu. Mutation alanı expected_revision,
resource alanı revision olarak birleştirildi. Organization dilinin tek kaynağı
default_locale oldu. Redis'in koşullu kullanımı master planda tutarlılaştırıldı.
Yeni ürün davranışı veya yayımlanmış API değişikliği yapılmadı.

## Doğrulama planı

Frontend: format/lint/typecheck/unit/build; tr-TR/en, 360/1280 px, klavye,
error, tema ve SSR izolasyonu için Playwright.
Backend: format/clippy/unit/HTTP testleri, OpenAPI drift, gerçek PostgreSQL'de
readiness ve migration up/repeat/revert/reapply/checksum testleri.
Docker: ayrı Compose project ve yeni volume ile build/up/wait, HTTP probe'ları,
DB kesintisinde readiness failure ve yeniden bağlantı sonrası recovery.
CI: aynı repository komutlarıyla frontend/backend/DB/contract ve Docker smoke.

## Resmî teknik kaynaklar

- [SvelteKit adapter-node](https://svelte.dev/docs/kit/adapter-node)
- [Axum](https://docs.rs/axum/0.8.8/axum/)
- [SQLx izole database testleri](https://docs.rs/sqlx/0.8.6/sqlx/attr.test.html)

README bakım değerlendirmesi: Kurulum/çalıştırma/test/migration/build değişiyor;
README aynı değişiklikte gerçek doğrulama sonuçlarına göre güncellenmelidir.
