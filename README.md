# Workflow & Operations Platform

Genel amaçlı, multi-tenant iş ve operasyon platformu. Mimari modular monolith;
PostgreSQL doğruluk kaynağıdır. Varsayılan arayüz dili tr-TR, ikinci dil en.

**Durum:** Generic SaaS foundation ve starter extraction tamamlandı.
Projects (STEP 17), recursive Sections (STEP 18), Work Items (STEP 19) ve
STEP 19.5 frontend ürün deneyimi mevcut. STEP 20 Process tanımları ve
STEP 21A Process execution core (başlat/tamamla/iptal, immutable deneme
geçmişi) yerel review aşamasında. STEP 21B derived Progress Engine
project/section/work-item yanıtlarına gömülüdür. STEP 21C assignment
(süreç sorumlusu + yürütme snapshot'ı) yerel review aşamasında.
Pause/resume ve realtime henüz uygulanmadı.

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
- [Users/sessions auth](docs/decisions/0004-users-sessions-auth.md)
- [Reusable SaaS Starter kararı](docs/decisions/0005-reusable-saas-starter.md)
- [Organizations/memberships](docs/decisions/0006-organizations-memberships.md)
- [Workspaces/memberships](docs/decisions/0007-workspaces-memberships.md)
- [RBAC authorization](docs/decisions/0008-rbac-authorization.md)
- [User invitations](docs/decisions/0009-user-invitations.md)
- [Minimal app shell](docs/decisions/0010-minimal-app-shell.md)
- [Project domain foundation](docs/decisions/0011-project-domain-foundation.md)
- [Sections / recursive hierarchy](docs/decisions/0012-sections-recursive-hierarchy.md)
- [Work Items foundation](docs/decisions/0013-work-items-domain-foundation.md)
- [Frontend ürün deneyimi](docs/decisions/0014-frontend-product-experience.md)
- [Process domain foundation](docs/decisions/0015-process-domain-foundation.md)
- [Process execution](docs/decisions/0016-process-execution.md)
- [Progress engine](docs/decisions/0017-progress-engine.md)
- [Assignment domain](docs/decisions/0018-assignment-domain.md)
- [Time sessions](docs/decisions/0019-time-sessions.md)

## Mevcut yapı

```text
apps/web/             SvelteKit + TypeScript, request-local i18n, tema ve hata görünümü
apps/server/          Axum, SQLx pool, config, JSON logging, probe ve migration araçları
apps/worker/          Henüz uygulanmadı; gerçek background iş geldiğinde açılacak
packages/ui/          Semantic tokenlar, PageHeader, StatusBadge, EmptyState, ConfirmDialog,
                      FormDrawer, ActionMenu
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
`http://127.0.0.1:8080`. Vite, `/api` isteklerini backend'e proxy'ler
(`API_PROXY_TARGET` ile değiştirilir); SSR kendi API_ORIGIN adresini kullanır.

### Authentication (users/sessions)

İlk kullanıcı `user-admin` ile oluşturulur (parola yalnız `USER_PASSWORD`
environment değişkeninden; argumente/log'a girmez):

```powershell
$env:USER_PASSWORD = 'guclu-bir-parola'
npm run user:create -- admin@example.test 'Yönetici'
```

Ardından `http://localhost:5173/login` ekranından giriş yapılır. Kök sayfa
kimlik doğrulama ister; üst barda kullanıcı adı ve çıkış düğmesi görünür.

- `POST /api/v1/auth/login`: Argon2id doğrulama; başarılıysa `platform_session`
  HttpOnly cookie'si set edilir (SameSite=Lax, Path=/, Max-Age=SESSION_TTL).
- `GET /api/v1/auth/me`: geçerli oturumla kullanıcı bilgisi; oturumsuz 401
  `AUTH_REQUIRED`.
- `POST /api/v1/auth/logout`: oturumu sunucuda geçersiz kılar, cookie'yi temizler.
- Bilinmeyen e-posta, yanlış parola ve disabled hesap aynı 401
  `AUTH_INVALID_CREDENTIALS` gövdesini döndürür (hesap varlığı sızdırılmaz).
- Session token DB'de yalnız SHA-256 digest olarak saklanır; süre DB saatinden
  hesaplanır ve logout revoked işaretler. Public register ve
  forgot/reset/verify endpoint'leri henüz yok (bkz. karar kaydı 0004).

- `GET /api/v1/health`: HTTP süreci canlıysa 200.
- `GET /api/v1/ready`: gerçek PostgreSQL SELECT 1 başarılıysa 200, aksi halde 503.
- Readiness migration sürümünü ölçmez; migration verify ayrı deployment kapısıdır.
- Hatalar stable code + request_id taşır; DB URL ve hata ayrıntısı açığa çıkmaz.
- Shutdown Ctrl+C/SIGTERM ile yönetilir.

## Organizations

Kimlik doğrulamış kullanıcı ilk organizasyonunu UI'daki tek alanlı formla
(`POST /api/v1/organizations`) oluşturur; organizasyon ve yaratıcı üyeliği
tek transaction'da yazılır. `GET /api/v1/organizations` ve
`GET /api/v1/organizations/{id}` yalnız kullanıcının aktif üyeliklerinin
gördüğü organizasyonları döndürür; üye olunmayan/bilinmeyen/silinmiş
durumlar aynı 404 gövdesiyle reddedilir (existency sızıntısı yok). Slug
otomatik türetilir veya normalize edilir; alınmış slug 422 "Already taken".
Header'daki organizasyon seçici yalnız presentation context'idir (cookie);
backend her istekte üyeliği yeniden doğrular. Slug asla authorization
sınırı değildir.

## Davetiyeler

Owner rolündeki üye (`members:invite` izni) yeni üye davet edebilir: `POST
/api/v1/organizations/{id}/invitations` yanıtında raw token bir kez döner
(email altyapısı sonraki fazda). Kabul: `POST /api/v1/invitations/accept`
body'de token — kimlik doğrulanmış + citext-eşit email şartıyla atomik
transaction'da membership (create/reactivate) + builtin `member` rolü. Tüm
reddedilen kabul denemeleri tek generic `422 INVITATION_INVALID` döner
(enumeration yok). Re-invite eski pending token'ı revoke eder. Owner
davetiyesi ve istemci-seçili rol YOKTUR. Detaylar ADR 0009'da.

## Roller ve izinler (RBAC)

Organizasyon yaratma, builtin 'owner'/'member' rollerini ve yaratıcının
Owner atamasını tek transaction'da oluşturur. Pre-RBAC organizasyonlara
migration builtin'leri ekler ama **otomatik Owner atamaz** (creator kaydı
yoktur; sıra tabanlı tahmin yetki yükseltmesi olurdu) — açık bootstrap
gerekir:

```powershell
npm run user:grant-owner -- <organization_id> <email>
```

Komut yalnızca organizasyonun aktif üyesine izin verir ve idempotentialdır
(ADR 0008). Yetkilendirme
permission anahtarlarıyla (`workspaces:create`) değerlendirilir — role
adları asla authorization girdisi değildir. `POST .../workspaces` için:
üye olmayan 404; üye ama izinsiz **403 PERMISSION_DENIED**; izinli üye 201.
Üyelik aktifliği her değerlendirmede yeniden doğrulanır; membership
deactivation atamaları fiziksel olarak siler → reactivation eski
ayrıcalıkları diriltmez. `GET .../permissions` ve `GET .../roles` her üyeye
açık katalog okumalarıdır. Çoklu rol → izin birleşimi.

## Workspaces

Seçili organization altında workspace'ler oluşturulur (`POST
/api/v1/organizations/{id}/workspaces`; workspace + creator membership tek
transaction'da). Erişim modeli: workspace'e erişim için geçerli organization
membership **ve** geçerli workspace membership birlikte gerekir; org üyeliği
tek başına workspace erişimi vermez. Parent-child path authoritativedir —
yanlış org/workspace kombinasyonu, foreign/unknown/malformed id ve silinmiş
kaynaklar aynı 404 ile reddedilir. Slug uniqueness organization başınadır.
`/auth/me` değişmedi; workspace'ler seçili org için liste endpoint'inden
alınır. Header'daki workspace seçici presentation context'tir; SSR,
sunucudan dönen permitted liste ile doğrular ve org değişiminde stale
seçim taşınmaz. Ayrıntılar ADR 0007'de.

## Projeler

İlk ürün-domain modülü (ADR 0011). Route parent zinciri authoritativedir:
`/api/v1/organizations/{org}/workspaces/{ws}/projects[/{project}]` — project
id tek başına hiçbir şey çözmez; yanlış parent kombinasyonu uniform 404'dür.
DB katmanında `FOREIGN KEY (tenant_id, workspace_id) → workspaces(tenant_id,
id)` cross-tenant satırı depolanamaz kılar. Okuma eligibility-based (aktif
org + workspace üyeliği); mutasyonlar `projects:create` / `projects:update`
izinleriyle, `status: "archived"` set eden PATCH'ler ek olarak
`projects:archive` ister. Status yaşam döngüsü: active/completed/archived
(geçiş modeli ADR 0011'de). Slug uniqueness workspace başınadır (citext,
case-insensitive). Create/update transaction içinde membership + permission
yeniden doğrular (TOCTOU). UI: `/app/{org}/{ws}/projects` — liste, boş durum,
permission-aware create formu; detay sayfası düzenleme + yaşam döngüsü
kontrolleri ve gelecekteki Bölümler için yer tutucu. DELETE/restore
endpoint'i, ürün semantiği tanımlanana kadar ertelendi.

## Bölümler (Sections)

Projelerin içinde genel amaçlı, keyfi derinlikte bölüm hiyerarşisi (ADR
0012): "Blok", "Kat", "Daire", "Bölge" yalnız kullanıcı seçimi isimlerdir —
tip/kolon DEĞİLDİR. Route: `.../projects/{p}/sections[/{s}]`; SectionContext
tüm üst zinciri (org → ws → project) tekil çözümde kanıtlar, yanlış-parent
uniform 404. DB: kompozit self-FK ebeveynin aynı tenant/workspace/project
üçlüsünü taşımasını ZORUNLU kılar (cross-project parenting depolanamaz);
döngü (self + torun-ebeveyn) mutasyon transaction'ında recursive CTE ile
reddedilir — aynı projenin mutasyonları projects satır kilidiyle serileşir.
Liste yanıtı DÜZ ve deterministik sıralı tek sorgudur (N+1 yok); istemci
hiyerarşiyi bellekte çözer (güvenli yardımcılar, bozuk girdi toleranslı).
Sıralama integer `position` primitive'ı (drag/drop ve gelecek bulk üretim
aynı primitive'ı kullanır; `(position, id)` deterministik). Slug tekliği
kardeş kapsamındadır. Yaşam döngüsü yapısal `active|archived`; arşiv yalnız
`sections:archive` yetkiyle. DELETE/bulk üretim ertelendi. UI: URL tabanlı
drill-down gezinme (ADR 0014) — proje sayfası yalnız kök section kartlarını
gösterir; her section kendi sayfasında (`.../sections/{section}`) doğrudan
alt bölümlerini ve işçiliklerini listeler; tüm torunlar tek sayfada açılmaz.
Yönetim işlemleri (yeniden adlandır/taşı/yukarı-aşağı/arşivle/yeniden
etkinleştir) section'ın kendi sayfasındadır; oluşturma mevcut bağlamın
altında yapılır. Breadcrumb üst zinciri taşır.

## İşçilik (Work Items)

İşçilikler sahibi section'ın sayfasında listelenir ve oluşturulur; detay
bağlantısı, düzenleme ve arşivleme mevcuttur. Route:
`/app/{org}/{ws}/projects/{project}/sections/{section}/work-items/{item}`;
eski `.../work-items` liste URL'si section sayfasına yönlenir.
Backend tam üst zinciri ve üyelikleri doğrular; create/update/archive ayrı
`work_items:*` izinleriyle çalışır. Arşiv normal list/get'ten düşer, satır ve
slug korunur; restore/move henüz yoktur. Process veya timer içermez.

`npm run test:db` yeni `work_items` target'ını da içerir. E2E runner bir
izole Work Items kullanıcısı ekler ve `E2E_COMPOSE_PROJECT` değerini yalnız
kendi geçici Compose fixture'ını hazırlayan teste iletir; geliştirici bu
değişkeni ayarlamaz. Üye test fixture'ı yalnız bu geçici veritabanında kurulur.

## Süreçler (Processes)

Süreçler bir işçiliğin sıralı **tanımlarıdır** (ör. Taş Alımı → Kesim →
İmalat → Nakliye → Montaj) ve işçilik detay sayfasındaki "Süreçler"
bölümünde yönetilir: oluştur/düzenle (drawer), zorunlu/opsiyonel, erişilebilir
yukarı/aşağı sıralama ve onaylı arşiv. `processes:create/update/archive/
reorder` ayrı izinlerdir; sıra sunucu tarafında tam permütasyon olarak
doğrulanır ve aktif pozisyonlar DB seviyesinde tekildir (ADR 0015).

Yürütme (STEP 21A, ADR 0016) ayrı kayıtlardır: `process_executions`
tablosu her denemeyi (attempt) immutable saklar — `active | completed |
cancelled`, işlem başına en fazla bir aktif deneme, pending = aktif deneme
yoktur. Başlat/Tamamla/İptal ve Tekrar Başlat işçilik sayfasındaki süreç
satırındadır; iptal isteğe bağlı nedeniyle onaylanır. Zamanlar PostgreSQL
`now()` ile yazılır — canlı timer yalnızca görüntüdür, `started_at` +
`server_time` anchor'ıyla yerel tick eder (saniyede yazma yok).
`process_executions:start/complete/cancel` ayrı izinlerdir; Owner ve
yerleşik Member üçüne de organization-scope sahiptir (Member'a
administrative yetki verilmez, workspace üyeliği yine şarttır). Aktif
deneme varken process/work item/section (alt ağaç dahil)/project arşivi
409 ile reddedilir. `npm run test:db` `process_executions` target'ını
içerir; E2E runner izole bir Processes kullanıcısı ekler.

İlerleme (STEP 21B, ADR 0017) saklanmaz, her yanıtta tek SQL ifadesiyle
türetilir: `progress: {completed, active, total, percent}` —
`percent` `total=0`'da `null`'dır. "Counted" = silinmemiş/aktif tanım ve
ulaşılabilir sahipler; DONE = completed denemesi var VE aktif deneme
yok — aktif retry yüzdeyi geriletir. Section/Project aggregate'leri tüm
alt ağaç üzerinden leaf-weighted'tır (child yüzde ortalaması değil);
yapısal değişmezlik garantidir. `npm run test:db` `progress` target'ını
içerir; E2E runner izole bir Progress kullanıcısı ekler.

Atama (STEP 21C, ADR 0018) süreçten kim **sorumlu** sorusunu yanıtlar ve
yürütme yetkisinden ayrıdır: `processes.assignee_user_id` mevcut sorumluyu
tutar (nullable, `PUT .../processes/{id}/assignment` ile `{"user_id": uuid
| null}` değişir); `process_executions.assignee_user_id` ise deneme
başlarken çekilen **değişmez snapshot**'tır — yeniden atama geçmiş denemeyi
yeniden yazmaz ve `started_by`/`completed_by`/`cancelled_by` aktörleri
bağımsız kalır (süpervizör başkasının işini başlatabilir). `processes:assign`
yalnızca Owner'a grant'lidir; Member atamayı görür ama kontrolü göremez.
Assignee uygunluğu composite FK `(workspace_id, user_id) →
workspace_memberships` ile şemada, transaction içinde aktif user + aktif
org + workspace üyeliği yeniden kanıtlanarak korunur; üyelik iptali saklı
atamayı temizlemez (stale, `eligible: false`). Üye seçici
`GET .../workspaces/{id}/members` dizininden beslenir (sadece `id +
display_name`, salt-okunur). `npm run test:db` `assignments` target'ını
içerir.

Time sessions (STEP 21D, ADR 0019) emek süresini yürütme duvar saatinden
ayırır: `process_execution_time_sessions` satırları, bir işçinin bir
denemede kesintisiz çalıştığı aralıklardır; duraklatma satırı kapatır,
devam yeni satır açar (paused durumu yok). `POST .../executions/{id}/
time-sessions` kendi oturumunu açar (worker = oturum açan kullanıcı;
V1 self-service), `POST .../{session_id}/stop` yalnızca kendi açık
oturumunu kapatır, `GET .../work-items/{id}/time-sessions` iş kalemindeki
açık oturumları döner ("şu an kim çalışıyor"). Bir worker tenant başına
en fazla bir açık oturum tutar (`ACTIVE_SESSION_EXISTS` 409); bir
denemede birden çok worker eşzamanlı çalışabilir. Yürütmenin
COMPLETE/CANCEL'i açık oturumları aynı transaction'da `ended_by =
geçiş aktörü` ile kapatır. `npm run test:db` `time_sessions` target'ını
içerir.

## Dil ve tema foundation

Çeviri anahtarları `apps/web/src/lib/i18n` altında; Svelte metinleri sözlükten
çözülür. `resolveLocale(user, organization)` sırası user → organization → tr-TR.
Henüz kullanıcı/organization modeli olmadığından doğrulanan `locale` cookie'si
presentation tercihi olarak kullanılır; tenant veya authorization verisi değildir.

Tema light varsayılan; `theme` cookie'si light/dark/system destekler. Tercih UI'si
henüz yoktur. Semantik tokenlar `packages/ui/src/tokens.css` içindedir. Sayfa
bileşenleri raw renk veya sektör kavramları taşımaz. SSR dil durumu global değildir.

## Environment değişkenleri

| Değişken                    | Anlam                                                                 |
| --------------------------- | --------------------------------------------------------------------- |
| POSTGRES_USER               | Yerel PostgreSQL bootstrap kullanıcısı; varsayılan platform           |
| POSTGRES_PASSWORD           | Yerel rastgele parola; gerçek değer commit/log edilmez                |
| POSTGRES_DB                 | Yerel development DB; varsayılan platform_dev                         |
| POSTGRES_PORT               | Host DB portu; varsayılan 15432                                       |
| DATABASE_URL                | Native server/migration bağlantısı; PostgreSQL URL zorunlu            |
| TEST_DATABASE_URL           | SQLx'in izole test DB'lerini oluşturabildiği ayrı test bağlantısı     |
| DATABASE_MAX_CONNECTIONS    | 1–100 arası pool limiti; varsayılan 10                                |
| SERVER_BIND                 | Native bind; varsayılan 127.0.0.1:8080                                |
| RUST_LOG                    | Tracing filtresi; varsayılan platform_server=info                     |
| API_PORT                    | Compose host API portu; varsayılan 8080                               |
| WEB_PORT                    | Compose host frontend portu; varsayılan 3000                          |
| SESSION_COOKIE_SECURE       | Session cookie Secure bayrağı; kod varsayılanı true, yerel .env false |
| SESSION_TTL_HOURS           | Oturum süresi; varsayılan 12 saat (1–720)                             |
| ARGON2_M_COST/T_COST/P_COST | Password hash parametreleri; varsayılan 19456/2/1 (OWASP)             |
| USER_PASSWORD               | `user-admin create` için tek kullanımlık parola kaynağı; loglanmaz    |
| API_PROXY_TARGET            | Vite dev `/api` proxy hedefi; varsayılan http://127.0.0.1:8080        |
| API_ORIGIN                  | SSR'nin backend'e erişim adresi; varsayılan http://127.0.0.1:8080     |

Port veya kimlik bilgisi değiştirilirse native DATABASE_URL ve TEST_DATABASE_URL
birlikte güncellenmelidir. Yerel bootstrap kullanıcısı production least-privilege
rol politikası değildir. Production secrets ve deployment bu adımın kapsamında değil.
`PLAYWRIGHT_CHANNEL=chrome` yalnızca testte kurulu Chrome seçimi için kullanılabilir.

## Migration disiplini

Mevcut migration'lar: `001` citext; `002` users + sessions; `003`
organizations + organization_memberships (ADR 0006); `004` workspaces +
workspace_memberships (ADR 0007); `005` RBAC — permissions/roles/
role_permissions/membership_roles + composite FK'ler + pre-RBAC org'lar
için (ADR 0008); `006` invitations + `members:invite` + Owner grant (ADR
0009); `007` projects — workspace-scoped ilk ürün domain tablosu, composite
FK + slug/status CHECK'leri + `projects:*` izinleri ve Owner grant backfill
(ADR 0011); `008` sections — genel recursive hiyerarşi, composite self-FK +
kardeş-kapsamı slug partial unique + `sections:*` izinleri ve Owner grant
backfill (ADR 0012); `009` work_items — section-scoped composite FK, hard slug
unique, ordering ve `work_items:*` grant backfill (ADR 0013); `010` processes —
work-item-scoped composite FK, hard slug unique, aktif pozisyon partial unique
ve `processes:*` grant backfill (ADR 0015); `011` process_executions —
immutable deneme kayıtları, composite process FK, tek aktif deneme partial
unique, terminal-state CHECK'leri ve `process_executions:*` grant backfill
(Owner + Member, organization scope) (ADR 0016); `012` progress destek
indexi — `process_executions(process_id) WHERE status='completed'`
(ADR 0017). SQLx
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

Tam stack browser testleri (izole Compose ortamı: PostgreSQL + migration +
backend, 28081/25433 portlarında; `user-admin` ile seed; sonunda teardown):

```powershell
npm run test:e2e
```

Script kendi Compose project'ini yönetir; çalışırken Docker Engine gerekir.
Windows'ta kurulu Chrome kullanımı:

```powershell
$env:PLAYWRIGHT_CHANNEL = 'chrome'
npm run test:e2e
```

Testler tr-TR/en, 360/375/768/1280 px, klavye, 404, tema, paralel SSR dil izolasyonu
ve gerçek auth akışını (login, yanlış parola, HttpOnly cookie, logout sonrası
oturum geçersizliği) kapsar. Project → Section → Work Item gezinmesi, deep link,
yetkisiz member, draft korunması ve arşiv onayı da gerçek backend üzerinde
doğrulanır. İzole runner altı sentetik kullanıcı oluşturur; experience testleri
kendi organizasyon/workspace verilerini oluşturur. Süreç/TV akışı henüz yoktur.

### Pre-push kapısı

`git push` öncesi hızlı yerel kalite kapısı (AGENTS.md §42). Her clone için
bir kez aktive edilir:

```powershell
npm run setup:hooks
```

Aktive edildikten sonra her push, `.githooks/pre-push` üzerinden
`npm run check:prepush` çalıştırır ve herhangi bir adım başarısızsa push'u
durdurur. Sıra (en ucuz/hızlıdan pahalıya):

```text
cargo fmt --all --check
npm run format:check
npm run lint
npm run typecheck
npm test                       (vitest unit)
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
npm run contracts:check        (OpenAPI/TypeScript drift)
```

Kapı bilinçli olarak ağır işleri içermez; PostgreSQL entegrasyon testleri
(`test:db`), Docker image build/smoke (`test:docker`) ve tam stack E2E
(`test:e2e`) CI'da ve tam yerel doğrulama akışında çalışır. Yeni dependency
eklenmedi; hook mevcut npm script'lerinden oluşur.

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

`.github/workflows/ci.yml` beş iş çalıştırır ve yerel doğrulama komutlarının
aynılarını kullanır: frontend (format/lint/typecheck/unit/build), e2e (Docker
stack üzerinde Playwright), backend (Rust fmt/clippy/test + OpenAPI/TypeScript
contract drift), database (`postgres:16-alpine` service üzerinde `test:db`
integration) ve docker (`test:docker` smoke). Faz 1 kapanışında (commit
78a7b4d) beş iş de hosted'da yeşildi.

## Sonraki aşama

STEP 19.5 final design/security/test quality review. Commit/push bu uygulama
görevinin kapsamında değildir. Process domain'i ayrı milestone'dır.
