# Workflow & Operations Platform

Genel amaçlı, multi-tenant iş ve operasyon platformu. Mimari modular monolith;
PostgreSQL doğruluk kaynağıdır. Varsayılan arayüz dili tr-TR, ikinci dil en.

**Durum:** First Agent Mission 2–6 altyapı çalışması tamamlandı. SvelteKit ve
Axum foundation, gerçek PostgreSQL migration kontrolleri ve Docker doğrulaması
çalışıyor; CI workflow oluşturuldu (repo henüz commit/uzak repositoryye sahip
olmadığından hosted CI çalıştırması bekliyor). Auth, users/sessions, tenant
tabloları ve domain özellikleri henüz uygulanmadı.

## Bağlayıcı belgeler

- [AGENTS.md](AGENTS.md): çalışma ve kalite kuralları
- [Master plan](docs/AI_CODING_AGENT_MASTER_PLAN.md): faz sırası
- [Veri modeli](docs/DATABASE_SCHEMA.md)
- [API sözleşmesi](docs/API_CONTRACT.md)
- [UX/UI](docs/UX_UI_SPEC.md)
- [Tasarım sistemi](docs/DESIGN_SYSTEM.md)
- [Başlangıç incelemesi](docs/decisions/0001-bootstrap-review.md)
- [Foundation kararları](docs/decisions/0002-platform-foundation.md)
- [Docker/CI doğrulaması](docs/decisions/0003-docker-ci-verification.md)

## Mevcut yapı

```text
apps/web/             SvelteKit + TypeScript, request-local i18n, tema ve hata görünümü
apps/server/          Axum, SQLx pool, config, JSON logging, probe ve migration araçları
apps/worker/          Henüz uygulanmadı; gerçek background iş geldiğinde açılacak
packages/ui/          Ortak semantic CSS tokenları
packages/contracts/   Rust'tan üretilen OpenAPI ve TypeScript şeması
migrations/           SQLx up/down migration çiftleri
scripts/              Environment, Cargo, contract ve Docker doğrulama komutları
docker/               Yerel image tarifleri
infrastructure/       Henüz ek altyapı tanımı yok
docs/                 Canonical sözleşmeler ve karar kayıtları
```

## Gereksinimler ve kurulum

- Node.js 24.x, npm 11.x, Git.
- Native backend için Rust 1.92.0; rustfmt/clippy `rust-toolchain.toml` ile sabit.
  Windows'ta MSVC C++ build araçları gerekir.
- PostgreSQL 16; yerel Docker akışı için çalışan Docker Engine/Desktop ve Compose v2+.
- Browser testleri için Chromium veya kurulu Chrome.

Repository kökünde:

```powershell
npm ci --include=optional --no-audit --no-fund
npm run setup:env
```

`setup:env` yalnızca `.env` yoksa rastgele yerel DB parolası üretir; mevcut
ayarları değiştirmez ve parolayı yazdırmaz. `.env` Git ve Docker build context
dışında tutulur. `.env.example` boş secret alanlarıyla referanstır.

## Native development

Veritabanı çalışırken:

```powershell
npm run db:migrate
npm run db:verify
npm run dev:server
```

Ayrı terminalde:

```powershell
npm run dev
```

Frontend Vite varsayılan adresi `http://localhost:5173`, backend adresi
`http://127.0.0.1:8080`. Domain bağlantısı veya authenticated app shell yoktur.
Frontend kök sayfası yalnızca ürün adını gösterir; demo dashboard değildir.

- `GET /api/v1/health`: HTTP süreci canlıysa 200.
- `GET /api/v1/ready`: gerçek PostgreSQL SELECT 1 başarılıysa 200, aksi halde 503.
- Readiness migration sürümünü ölçmez; migration verify ayrı deployment kapısıdır.
- Hatalar stable code + request_id taşır; DB URL ve hata ayrıntısı açığa çıkmaz.
- Shutdown Ctrl+C/SIGTERM ile yönetilir.

## Dil ve tema foundation

Çeviri anahtarları `apps/web/src/lib/i18n` altında; Svelte metinleri sözlükten
çözülür. `resolveLocale(user, organization)` sırası user → organization → tr-TR.
Henüz kullanıcı/organization modeli olmadığından doğrulanan `locale` cookie'si
presentation tercihi olarak kullanılır; tenant veya authorization verisi değildir.

Tema light varsayılan; `theme` cookie'si light/dark/system destekler. Tercih UI'si
henüz yoktur. Semantik tokenlar `packages/ui/src/tokens.css` içindedir. Sayfa
bileşenleri raw renk veya sektör kavramları taşımaz. SSR dil durumu global değildir.

## Environment değişkenleri

| Değişken                 | Anlam                                                             |
| ------------------------ | ----------------------------------------------------------------- |
| POSTGRES_USER            | Yerel PostgreSQL bootstrap kullanıcısı; varsayılan platform       |
| POSTGRES_PASSWORD        | Yerel rastgele parola; gerçek değer commit/log edilmez            |
| POSTGRES_DB              | Yerel development DB; varsayılan platform_dev                     |
| POSTGRES_PORT            | Host DB portu; varsayılan 15432                                   |
| DATABASE_URL             | Native server/migration bağlantısı; PostgreSQL URL zorunlu        |
| TEST_DATABASE_URL        | SQLx'in izole test DB'lerini oluşturabildiği ayrı test bağlantısı |
| DATABASE_MAX_CONNECTIONS | 1–100 arası pool limiti; varsayılan 10                            |
| SERVER_BIND              | Native bind; varsayılan 127.0.0.1:8080                            |
| RUST_LOG                 | Tracing filtresi; varsayılan platform_server=info                 |
| API_PORT                 | Compose host API portu; varsayılan 8080                           |
| WEB_PORT                 | Compose host frontend portu; varsayılan 3000                      |

Port veya kimlik bilgisi değiştirilirse native DATABASE_URL ve TEST_DATABASE_URL
birlikte güncellenmelidir. Yerel bootstrap kullanıcısı production least-privilege
rol politikası değildir. Production secrets ve deployment bu adımın kapsamında değil.
`PLAYWRIGHT_CHANNEL=chrome` yalnızca testte kurulu Chrome seçimi için kullanılabilir.

## Migration disiplini

Tek infrastructure migration `citext` kurar; domain tablosu yaratmaz. SQLx
`_sqlx_migrations` tablosunda version/checksum tutar. Uygulanmış SQL dosyası
sonradan değiştirilmez; yeni migration eklenir. Dosyalar LF satır sonuyla tutulur.

```powershell
npm run db:migrate
npm run db:verify
npm run db:migration:add -- descriptive_snake_case_name
```

Generator boş up/down SQL çiftini oluşturur; SQL ve testler eklenmeden migration
hazır sayılmaz. Rebuild, migration klasöründeki değişiklikleri izler.

Yalnızca güvenli olduğu incelenmiş disposable development veritabanında:

```powershell
npm run db:revert
npm run db:migrate
```

Revert en son migration'ı geri alır ve güncel/checksum-uyumlu tarihçe gerektirir.
Down SQL CASCADE kullanmaz; bağımlı tablo varsa veri silmek yerine başarısız olur.
Production düzeltmeleri tercihen yeni forward migration ile yapılır.

## Doğrulama komutları

```powershell
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
npm run check:server
npm run test:db
npm run contracts:check
```

`test:db`, TEST_DATABASE_URL üzerinden SQLx'in ayrı temporary DB'lerinde çalışır;
CI'da zorunludur. Database olmadan sadece unit/HTTP testleri için
`npm run test:server` kullanılır. Entegrasyon testi sessizce atlanmaz; ayrı feature
ve komutla çağrılır.

Windows'ta kurulu Chrome ile doğrulanan browser komutu:

```powershell
$env:PLAYWRIGHT_CHANNEL = 'chrome'
npm run test:e2e
```

Linux/macOS eşdeğeri environment değişkeni aynı Playwright ayarına bağlanır.
Bu ortamda Playwright Chromium indirmesi zaman aşımına uğradı; kurulu Chrome
ile testler tamamlandı. Testler tr-TR/en, 360/1280 px, klavye, 404, tema ve
paralel SSR dil izolasyonunu kapsar. Ürün kritik E2E akışı henüz mevcut değildir.

Kod formatlamak ve sözleşme yenilemek için:

```powershell
npm run format
npm run contracts:generate
```

OpenAPI Rust'a aittir; generated JSON/TypeScript dosyalarını elle değiştirmeyin.
Contract check, Rust'tan yeniden üretilen şema ile iki dosyanın drift'ini denetler.

## Docker

Tek komutla yerel ortam (PostgreSQL 16 + migration + backend + frontend):

```powershell
npm run docker:up
npm run docker:down
```

`docker:up` gerekiyorsa image'leri örnekler, PostgreSQL healthy olana kadar
bekler, migration'ı çalıştırır, backend `ready` ve frontend healthy olana kadar
bekler. Host adresleri: API `http://127.0.0.1:8080`, frontend
`http://127.0.0.1:3000`, PostgreSQL `127.0.0.1:15432`. `docker:down`
container'ları ve ağı kaldırır; `postgres-data` volume'u korunur.

Doğrulanmış smoke testi — her çalıştırma taze Compose project ve volume
kullanır; health/ready probe'ları, migration verify, PostgreSQL kesintisinde
readiness 503 `SERVICE_NOT_READY`, veritabanı geri gelince readiness recovery ve
temiz teardown kontrol eder:

```powershell
npm run test:docker
```

Ayrıca her iki image tüm stage'leriyle (`docker build --no-cache`: server
build+runtime; web development+build+runtime) sıfırdan örneklenerek temiz
kurulum doğrulandı. Sonuçlar
[foundation karar kayıtlarında](docs/decisions/0003-docker-ci-verification.md)
tutulur.

## CI

`.github/workflows/ci.yml` dört iş çalıştırır ve yerel doğrulama komutlarının
aynılarını kullanır: frontend (format/lint/typecheck/unit/build + Playwright
e2e), backend (Rust fmt/clippy/test + OpenAPI/TypeScript contract drift),
database (`postgres:16-alpine` service üzerinde `test:db` integration) ve
docker (`test:docker` smoke). Workflow YAML sözdizimi doğrulandı; repository
henüz commit ve uzak repositoryye sahip olmadığından hosted çalıştırma
yapılmadı — ilk push'ta çalışacaktır.

## Sonraki aşama

Faz 1 kalite koşulları sağlandıktan sonra First Agent Mission adım 7:
users/sessions; ardından organizations, workspaces, tenant middleware ve
cross-tenant testler. Bu güvenlik kapıları geçmeden domain özelliklerine geçilmez.
