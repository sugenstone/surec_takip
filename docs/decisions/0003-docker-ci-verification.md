# Faz 1 kapanışı: Docker doğrulaması ve CI

Tarih: 2026-09-21

## Devralma incelemesi

Önceki oturum kullanım limiti nedeniyle Docker doğrulaması sırasında durdu.
Kod yazılmadan önce repository'nin gerçek durumu source of truth alındı:
canonical belgeler, karar kayıtları, git durumu (commit yok, tüm dosyalar
untracked), npm/Cargo workspace'ler, Docker/Compose, migration, testler ve
script'ler yeniden incelendi. Git "dubious ownership" koruması güvenli dizin
istisnasıyla aşıldı; repository içeriği değiştirilmedi.

Önceki agentın tamamladığı işler yerel komutlarla yeniden doğrulandı:
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets
--all-features --locked -- -D warnings`, `cargo test --workspace --locked`
(2 HTTP testi dahil), `npm run lint`, `npm test` (4 i18n testi),
`npm run contracts:check` geçti; Playwright son çalıştırması `passed`
(`test-results/.last-run.json`).

Tespit edilen yarım işler: `npm run format:check` README.md'te uyarı veriyordu
(son README güncellemesinden sonra format atlanmış); Docker smoke script'i
yazılmış ama sonucu kayda geçmemişti; `.github/workflows` hiç oluşturulmamıştı.

## Docker doğrulaması (tamamlandı)

- `npm run test:docker`: rastgele adlı taze Compose project + taze volume ile
  build/up/wait; PostgreSQL healthy; migrate up; `GET /api/v1/health` 200;
  `GET /api/v1/ready` 200; frontend `/` 200; `migrate verify` başarılı;
  `db` durdurulunca health 200 kalırken ready 503 `SERVICE_NOT_READY` döndü;
  `db` başlatılınca readiness 30 deneme içinde 200'e döndü; `down --volumes`
  temiz teardown. Geçti.
- Temiz build kanıtı: `docker build --no-cache` ile server image'ı (build +
  runtime stage'leri) ve web image'ı (development + build + runtime stage'leri)
  sıfırdan örneklendi. Geçti.
- Tek komutla başlatma (Faz 1 çıkış kriteri): `npm run docker:up` db healthy →
  migrate tamam → server healthy → web healthy sırasıyla bekledi;
  `127.0.0.1:8080/api/v1/health` 200, `/api/v1/ready` 200
  (`{"data":{"status":"ready"}}`), `127.0.0.1:3000/` 200; `npm run docker:down`
  container/ağı kaldırdı (`postgres-data` volume bilinçli olarak korunur).
  Geçti.

## CI

`.github/workflows/ci.yml` oluşturuldu; sözleşme gereği kapılar: Rust
format/lint/test, TypeScript/typecheck/frontend lint/frontend test, database
migration doğrulaması, contract drift ve Docker smoke. İşler:

- frontend: `format:check`, `lint`, `typecheck`, `test`, `build`,
  `playwright install chromium` + `test:e2e`.
- backend: Rust 1.92.0 fmt/clippy/test + `npm run contracts:check`
  (OpenAPI/TypeScript drift).
- database: `postgres:16-alpine` service (15432) üzerinde repo script'lerinin
  okuduğu `.env` sabit CI kimlik bilgileriyle üretilir, `npm run test:db`
  çalışır (SQLx izole geçici veritabanları).
- docker: `npm run test:docker`; `POSTGRES_PASSWORD` iş ortamından verilir.

YAML söz dizimi doğrulandı (`js-yaml` ile ayrıştırma başarılı). Repository
commit ve uzak repositoryye sahip olmadığından hosted çalıştırma yapılamadı;
ilk push'ta çalışacaktır. Bu sınır README'de açıkça belirtilir.

## Temizlik ve kapsam dışı

- Doğrulama amaçlı geçici image'ler (`surec-smoke-*`, `surec-cleancheck:*`)
  silindi; projenin `surec_takip-*` image'leri bırakıldı.
- README yalnızca bu oturumda çalıştırılmış komutlarla güncellendi ve Prettier
  drift'i giderildi (`npm run format:check` temiz).
- Kullanıcı talimatı gereği users/sessions, organizations, workspaces,
  projects/sections/cards ve yeni domain feature geliştirmesi başlatılmadı;
  mevcut kod yeniden yazılmadı, framework/mimari değiştirilmedi.
