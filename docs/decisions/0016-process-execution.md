# Process execution core (süreç yürütme çekirdeği)

Tarih: 2026-09-26
Durum: Tasarım kilitli; ürün kararları onaylandı (STEP 21A henüz uygulanmadı)

## Bağlam

STEP 20 (ADR 0015) `processes` tablosunu yalnız **tanım** (definition) olarak
kurdu: hangi süreçler var, sırası, zorunlu/opsiyonel. Bu ADR, tanımın
üzerine **yürütme** (execution) alanını tanımlar: ne başladı, ne bitti, kim
başlattı/bitirdi/iptal etti, kaçıncı deneme, ne kadar sürdü. Amaç alan
operasyonudur (Saha: Başlat/Tamamla/İptal/Tekrar Başlat); merkez ofisin canlı
aktif süreç görünümü 21E'de gelecektir.

Tüm kararlar Product Owner tarafından kilitlendi (D1–D18); bu ADR onları ve
mimari gerekçelerini kayıt altına alır.

## Tanım ≠ yürütme (D1)

`processes` satırı runtime durumu TAŞIMAZ. Yürütme ayrı
`process_executions` tablosudur ve kararlı `processes.id`'ye referans verir.
Tanım yeniden adlandırılsa, sıralansa veya arşivlense bile yürütme geçmişi
kimliğini ve gerçeğini korur. Execution alanlarını `processes`'e eklemek
(Alternatif B) reddedildi: tarihi yok eder, config/runtime'ı tek satırda
birleştirir, retry'yi belirsizleştirir.

## Çoklu deneme / immutable attempt (D2)

Bir tanımın 0..N yürütme denemesi olabilir; her deneme `attempt_no = 1,2,3…`
alır. Herhangi bir anda en fazla BİR `active` deneme vardır — DB bunu kısmi
unique index ile zorlar (`UNIQUE (process_id) WHERE status='active'`).
Terminal (`completed`/`cancelled`) denemeler IMMUTABLE'dır; "tekrar başlat /
reopen" yeni deneme satırıdır, eski satır asla active'e dönmez. Tarihsel
denemeler sorgulanabilir kalır (deneme sayısı, deneme başına süre, deneme
başına aktör).

## Durum makinesi (D3)

```
(hiç active yok = örtük pending)
        │  START
        ▼
     active ──COMPLETE──► completed (terminal, immutable)
        │
        └──CANCEL───────► cancelled (terminal, immutable)
```

- `pending` saklanmaz: active denemesi olmayan süreç zaten bekliyordur.
- `completed`/`cancelled` terminaldir; yeni START `attempt_no+1` üretir.
- V1'de `paused` YOK: duraklatma "zaman oturumu" (time session) kavramıdır
  ve 21D'de `time_sessions` ile gelecek — yürütme satırının `started_at`'i
  asla ileri/geri oynatılmaz.
- V1'de `failed`/`rejected`/`blocked`/`awaiting_approval` YOK; her biri
  kendi fazını bekler (bağımlılık/approval domain'leri).

## Komutlar ve route şekli

Açık domain komutları (AGENTS.md §12), tam parent zinciri altında:

```
POST …/work-items/{w}/processes/{p}/executions                → 201 START (yeni deneme)
GET  …/work-items/{w}/executions                              → 200 iş öğesinin tüm denemeleri
POST …/processes/{p}/executions/{e}/complete                  → 200 COMPLETE
POST …/processes/{p}/executions/{e}/cancel                    → 200 CANCEL
```

Global `/executions/{id}` YOKTUR. Yürütme asla yalnız id ile yetkilenmez;
`ProcessExecutionContext`, `ProcessContext`'in üzerine yürütmeyi tam scope +
`process_id` ile çözer. Yanlış parent/yabancı/arşivli/bilinmeyen hepsi aynı
404 parmak izini üretir.

Redo `process_executions:start` iznini kullanır (D4) — ayrı `reopen` izni
yoktur. Aynı aktör başlatıp bitirebilir (D8). Atanmamış süreç
başlatılabilir (D6). Pending doğrudan completed'a atlayamaz (D16): her
tamamlanan deneme START→ACTIVE→COMPLETE geçmişine sahiptir.

## Aktör modeli

`started_by_user_id`, `completed_by_user_id`, `cancelled_by_user_id`
kolonları `users(id)`'e FK'dır ve yalnız sunucu yazar (oturum aktörü).
İstemci hiçbir aktör/zaman alanı gönderemez. Aktör ≠ atanan (assignee):
atama 21C'nin konusudur ve 21A şemasına sahte assignee alanı eklenmez;
ileride `assignee_type ∈ {user, team}` + geçmişli atama tablosu eklenecek
şekilde uyumludur.

## Neden/not alanı (D15)

İki ayrı, sınırlı, nullable kolon: `start_reason text NULL` ve
`cancel_reason text NULL` (≤ 500 karakter). İsteğe bağlıdır; asla zorunlu
değildir. Tek generic `note` reddedildi: sahipliği belirsiz (başlangıç notu
mu, iptal notu mu?). `start_reason` yeniden-deneme gerekçesini,
`cancel_reason` terk etme gerekçesini taşır; complete'a V1'de not yoktur.

## Zaman ve timer (D17, D18)

- Yetkili saat: PostgreSQL `now()`, `timestamptz` UTC. İstemci zamanı
  hiçbir transition'a kabul edilmez.
- Persist edilen: `started_at`, `completed_at`, `cancelled_at` + aktörler.
  Persist EDİLMEYEN: elapsed, duration, heartbeat — tamamı türetilir.
- Aktif geçen süre = `now() − started_at` (sorgu zamanında); tamamlanan
  süre = `completed_at − started_at`; iptal edilen süre aynı şekilde.
- Saniyelik DB yazması YOKTUR. Frontend, cevaptaki `started_at` +
  `server_time` ile offset hesaplar ve yerel tick yapar; reload yeniden
  demirler; birden çok izleyici aynı `started_at`'e demirlenir; bozuk cihaz
  saati offset ile düzelir.
- Gerçek "çalışma süresi" (duraklamaları çıkaran) 21D `time_sessions`'tan
  toplanır; execution aralığı kaba süredir.

## İlerleme politikası (D9–D12) — 21B'de uygulanacak, burada kilitli

- Tanım DONE ⇔ en az bir `completed` denemesi VAR ∧ `active` deneme YOK.
- Work Item ilerlemesi = done aktif tanımlar / toplam aktif tanımlar.
- Arşivli/silinmiş tanımlar her iki taraftan da çıkarılır; `is_required`
  paydayı değiştirmez; iptal edilen deneme sayılmaz.
- Tamamlanmış tanım yeniden başlatılırsa ilerleme GERİLER (rework gerçeği,
  kasıtlı).
- Sıfır aktif tanım ⇒ progress = NULL ⇒ UI "—" (0% veya 100% değil).
- Roll-up LEAF-WEIGHTED: Section/Project ilerlemesi = alt ağaçtaki tanım
  sayıları üzerinden done/total; ortalama-ortalaması YOK. Aggregate tek bir
  domain fonksiyonu arkasında; politika değişimi tek yerden.
- Persist edilmiş yüzde kolonu YOK — türetme, stale-cache riski yok.

## Sıra ≠ bağımlılık (D13)

`position` görüntü/anlamsal sıradır; yürütmeyi engellemez. V1 herhangi bir
sürecin herhangi bir sırada başlatılmasına izin verir. Gerçek bağımlılık
(hard block / warn only / cycle / unlock) ayrı domain fazıdır (mantıksal
012); `position`'dan bağımlılık türetilmez.

## Arşiv kapısı (D14)

Aktif yürütme varken arşivleme/soft-delete ENGELLENİR — tanım, work item,
section ve project seviyesinde. Zombi/görünmez canlı timer yaratılmaz;
sessiz auto-cancel YOKTUR. İhlal `409 STATE_CONFLICT` döner.

- `processes` arşiv koruması 21A çekirdeğindedir (aynı domain).
- `work_items`/`sections`/`projects` arşiv korumaları ayrı bir 21A-sonrası
  uyumluluk dilimidir (21A.1): üç ayrı domain modülünü etkiler, kendi test
  matrisi vardır. Proje kilidi tüm bu yolları serileştirir; guard, proje
  kilidi altında `EXISTS (active execution in scope)` kontrolüdür.
- Section notu: context yalnız doğrudan section'ın `status='active'`
  olmasını ister; arşivlenen section'ın ALT section'ları kendi rotalarından
  erişilebilir kalır. Guard kapsamı = arşivlemeyle erişilemez olacak
  execution'lar (section için: doğrudan sahip olduğu work item'lar;
  proje için: tüm proje). Uygulama sırasında doğrulanacak.

## RBAC

Yeni izin anahtarları: `process_executions:start`, `:complete`, `:cancel`.
Okumalar eligibility tabanlı (çift üyelik), izin anahtarı gerektirmez —
mevcut konvansiyon.

- Owner backfill'i organization scope'ta (010 kalıbı).
- Member backfill'i: ürün kararı "normal saha Member'i başlat/bitir/iptal
  yapabilmeli". Depoda `member` built-in rolü org-genelinde atanır
  (`membership_roles.workspace_id IS NULL`, davet kabulünde). Bu yüzden
  Member grant'leri de `'organization'` scope'ta yazılır — workspace-scope
  grant, org-genel atamalarla eşleşmezdi. `membership_roles`'a DOKUNULMAZ;
  admin izinleri verilmez; workspace üyeliği eligibility'de zorunlu kalır.
- `bootstrap_builtin_roles` (rbac.rs) yeni organizasyonlar için aynı üç
  anahtarı hem owner'a hem member'a ekler.
- Bilinçli test değişikliği: `tests/processes.rs`'teki
  `member_has_no_grants` iddiası (member'da SIFIR grant) 011 sonrası üç
  execution anahtarını görecek — iddia "member yalnız execution anahtarları"
  olacak şekilde güncellenir. Zayıflatma değil, ürün kararının yansıması.

## Transaction / kilit sırası / TOCTOU

STEP 20 öneki değiştirilmeden uzatılır:

1. project `FOR UPDATE` — projedeki tüm yazmaları serileştirir;
2. direct section + org + workspace + iki üyelik `FOR SHARE`;
3. work item `FOR SHARE`;
4. izin destek satırları `FOR SHARE` (`lock_workspace_permission_in_tx`);
5. process satırı `FOR SHARE` (tanım okunur, yazılmaz);
6. execution satırları `FOR UPDATE` (transition komutlarında).

`attempt_no = max(attempt_no)+1` güvenlidir: tüm execution ve tanım
yazmaları aynı proje kilidini önce aldığından oku-sonra-yaz yarışı
oluşamaz; `UNIQUE (process_id, attempt_no)` yine de savunma olarak durur
(23503 → 409). `one_active` index START×START yarışını tek kazananla
bitirir. Tüm yollar aynı kilit sırasını izlediğinden deadlock yapısal
olarak imkânsızdır.

Hata sırası: 401 → 404 → 403 → 400/422 → **409 STATE_CONFLICT**
(yeni `ErrorCode` varyantı; API_CONTRACT'ın ayrılmış 409 anlamıyla uyumlu).
State kontrolü, kilitli satır üzerinde, izin ve alan doğrulamasından sonra
yapılır; yetkisiz + bozuk gövde yine 403 alır.

## Denetim borcu (kayıtlı)

`audit_events`/`outbox_events` henüz implemente edilmedi (mantıksal 004
atlandı). Immutable denemeler + aktör/zaman damgaları şimdilik operasyonel
tarih taşır ama genel audit log'unun yerine GEÇMEZ. Yürütme
transition'ları ileride aynı transaction içinde outbox event üretecek
(`process_execution.started|completed|cancelled`, payload = execution id +
scope + attempt_no + aktör + sunucu zamanı). Bu borç burada ve
DATABASE_SCHEMA'de açıkça kayıtlıdır.

## Yol haritası (kilitli)

- 21A Execution Core — bu ADR'nin konusu (tablo + START/COMPLETE/CANCEL +
  retry + güvenlik).
- 21B Progress Engine — türetilmiş WI/section/project ilerlemesi.
- 21C Assignment — tanım varsayılanı + execution ataması + geçmiş; user
  önce, team sonra.
- 21D Time Sessions — pause/resume; gerçek çalışma süresi; saniyelik yazma
  yok; execution `started_at` ASLA değişmez.
- 21E Active Processes — çapraz-proje canlı liste, uzun-koşan önce, filtre,
  deep link.
- 21F Execution UX Polish.
- Daha sonra: dependencies, realtime/outbox, TV, templates.

## Reddedilen alternatifler

- `processes` üzerinde mutable execution kolonları — tarih yok olur.
- Tek ömürlük execution / yerinde reopen — süre ve rework tarihi bozulur.
- Saklı `pending` satırları — gereksiz, sapma riski.
- Tam event-sourcing — audit fazının işi; V1 için erken.
- `position`'a dayalı otomatik gating — bağımlılık değildir.
- Parent arşivlemede auto-cancel — sessiz operasyonel yazma yaratır.
- Workspace-scope Member grant — org-genel atamalarla eşleşmez.

## Sonuçlar / uyumluluk

- Migration 011 tamamen ADDITIVE'dir: `processes` üzerine ek composite
  unique (FK hedefi) + `process_executions` + izin kataloğu/backfill.
  Migration 010 değiştirilmez. Down: tablo → grant → katalog → FK hedefi;
  CASCADE yok.
- Mevcut 001–010 davranışı değişmez; yeni alanlar/route'lar geriye dönük
  uyumludur. Member'a execution izni verilmesi tek bilinçli davranış
  değişimidir (ürün kararı).
- Yeni entegrasyon suite'i `tests/process_executions.rs`, Cargo.toml'da
  `required-features = ["integration"]` ile KAYITLI olmalıdır (STEP 20
  hosted-CI hatasının tekrarı yasak).
