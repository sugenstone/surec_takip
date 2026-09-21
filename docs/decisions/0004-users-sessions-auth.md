# First Agent Mission adım 7: users + sessions

Tarih: 2026-09-21

## Kapsam

Yalnızca authentication foundation: `users` ve `sessions` tabloları,
Argon2id password hashing, HttpOnly cookie session transportu,
`/api/v1/auth/login|logout|me` endpoint'leri, minimal login ekranı ve
logout. Organizations, workspaces, RBAC, domain feature'ları ve tenant
authorization bu dilimde yoktur; session kimlik doğrular, üyelik/rol
vermez. `me` yanıtı sözleşmedeki `organizations` alanını boş dizi olarak
taşır.

## Veri modeli ve migration

`20260921204143_users_sessions`: `users` (citext UNIQUE email, nullable
password_hash — gelecek SSO hesapları için canonical şemaya uygun,
PHC string hash), `sessions` (UNIQUE token_hash, `expires_at > created_at`
CHECK, user FK). Indexler: token_hash unique (her authenticated istekte
digest lookup), user_id, expires_at. Down migration CASCADE kullanmaz;
bağımlı nesne varsa başarısız olur (test edildi). Migration testleri iki
migration gerçeğine güncellendi: revert yalnızca en yenisini geri alır,
citext kalır; `users` FK'sına bağımlı probe rollback'i bloklar.

## Güvenlik kararları

- Password: Argon2id (PHC string). Parametreler merkezî `AuthConfig`'ten;
  OWASP varsayılanları m=19456 KiB, t=2, p=1; env ile sınırlar içinde
  ayarlanabilir. Hash/password hiçbir log, audit payload'ı, hata mesajı
  veya API yanıtına girmez (test edildi).
- Session token: 32 bayt OsRng → base64url. DB'de yalnızca SHA-256
  digest saklanır; raw token sadece HttpOnly cookie'da yaşar (test
  edildi). Expiry DB saatine göredir (`now()+ttl`, saat kayması yok);
  varsayılan 12 saat.
- Cookie: `platform_session`, `Path=/`, `HttpOnly`, `SameSite=Lax`,
  `Max-Age=ttl`; `Secure` kodda varsayılan true, yerel HTTP geliştirme
  için `.env`/Compose'ta false. Logout sunucuda `revoked_at` yazar ve
  cookie'yi `Max-Age=0` ile temizler; eski token tekrar kullanılamaz.
  Token her login'de sunucuda üretilir (fixasyon vektörü yok).
- Login ayırt edilemezliği: bilinmeyen e-posta, yanlış şifre, disabled
  hesap ve hash'siz hesap aynı `401 AUTH_INVALID_CREDENTIALS` gövdesini
  döndürür. Bilinmeyen e-postada dummy Argon2 verify çalışır; disabled
  hesapta gerçek verify status kontrolünden önce çalışır — zamanlama
  kanalı eşitlenir.
- Kullanıcı doğrulama ↔ tenant authorization karışmaz; extractor yalnızca
  `status=active` kullanıcıyı kabul eder.

## Sözleşme değişiklikleri

- Yeni stable hata kodları: `AUTH_INVALID_CREDENTIALS` (401),
  `INTERNAL_ERROR` (500); `API_CONTRACT.md` kataloğuna eklendi. Hatalı
  JSON gövdesi 400, alan doğrulaması 422 — ikisi de `VALIDATION_ERROR`.
- OpenAPI Rust'tan üretildi; `contracts:generate`/`check` akışı korundu.

## Belgelenen ertelemeler (ürün kararı gerektirenleler raporlandı)

- Public register/onboarding endpoint'i canonical sözleşmede tanımsız;
  davet akışı organizations fazında gelecek. İlk kullanıcı bootstrap'i
  `user-admin create` CLI'ı ile (password yalnız `USER_PASSWORD` env).
- `password/forgot|reset`, `email/verify` email altyapısı (Faz 15) ve
  token modeli tanımlanmadan uygulanmaz; `email_verified_at` NULL kalır
  ve login'i engellemez.
- Login rate limiting bu dilimde yok (Faz 1 CI altyapısında Redis/tabansal
  sayaç gerekir); Argon2 maliyeti kaba kuvveti yavaşlatır. Risk açık.
- Auth olayları için ayrı audit_events tablosu canonical audit fazında
  gelir; sessions tablosu bu aşamada session geçmişinin kendisidir.
  Hassas değerler audit'e asla girmez.

## Frontend ve E2E

- Vite dev `/api` proxy'si (üretimde reverse proxy aynı origine indirger);
  SSR `API_ORIGIN` üzerinden `/auth/me` ile session bootstrap eder, cookie
  bilgisini kendisi yorumlamaz. `/` korumalı, `/login` giriş yapmışsa
  yönlendirir. Görünür metinler tr-TR/en sözlüklerden; hata seçimi stable
  `code` üzerinden (mesaj metninden değil).
- `npm run test:e2e` artık izole Compose stack'i (db+migrate+server,
  28081/25433) ayağa kaldırır, `user-admin` ile kullanıcı eğer, Playwright'
  çalıştırır ve teardown eder. Foundation spec'leri korumalı kök sayfa
  gerçeğine güncellendi. CI'da e2e ayrı job (Docker daemon + chromium).

## Doğrulama (hepsi bu oturumda çalıştırıldı)

- `check:server`: cargo fmt --check, clippy `-D warnings`, test — 13 unit +
  4 HTTP testi geçti.
- `test:db` (gerçek PostgreSQL): 4 migration + 9 auth integration testi geçti
  (cookie öznitelikleri, raw token DB'de yok + digest eşleşmesi, ayırt
  edilemez 401'ler, disabled hesap, TTL SQL doğrulaması, tampered/expired/
  revoked session, logout sonrası kullanılamazlık, citext duplicate,
  concurrent tek kazanan).
- `contracts:generate` + `contracts:check`: drift yok.
- Manuel native smoke: user-admin create → login → me → yanlış şifre 401 →
  logout; JSON loglarda yalnız route şablonu.
- Frontend: format/lint/typecheck/unit (7)/build geçti.
- `test:e2e` (izole Compose stack + kurulu Chrome): 10/10 — 4 foundation
  (tr/en × 360/1280 klavye+404), tema, SSR dil izolasyonu, 4 auth (yönlendirme,
  yanlış parfa, HttpOnly cookie okunamıyor + logout revoke, en hata metni).
- `test:docker` (güncel Compose/Dockerfile ile): temiz volume smoke geçti.

E2E sırasında bulunan ve düzeltilen iki koşucu hatası kayda değer: (1)
`process.exit` finally teardown'unu atlıyordu (stack sızdırıyordu) —
`process.exitCode` kullanıldı; (2) Windows'ta `spawn('npx')` .cmd shim'i
başlatamıyordu — `process.execPath` ile `playwright/cli.js` doğrudan
çağrılıyor. Ayrıca dev server'da hydration öncesi native submit yarışını
önlemek için forma `method="post"` eklendi (kimlik bilgisi URL'ye asla
düşmez) ve testler hydration'ı bekliyor.
