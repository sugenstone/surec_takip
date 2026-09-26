# Assignment Domain (süreç sorumluluğu + yürütme snapshot'ı)

Tarih: 2026-09-28
Durum: Kabul edildi (STEP 21C)

## Bağlam

STEP 21A (ADR 0016) yürütme çekirdeğinde "kayıt hangi kullanıcı tarafından
başlatıldı/tamamlandı/iptal edildi" sorusunu `started_by`/`completed_by`/
`cancelled_by` aktör alanlarıyla çözdü. Eksik kalan ayrı bir sorudur:

> "Bu süreçten **kim sorumlu?**"

Atama (assignment) sorumluluk metadata'sıdır; yetkilendirme DEĞİLDİR —
`process_executions:*` izinlerine sahip herhangi bir kullanıcı, sorumlusu
kendisi olmayan bir süreci de başlatabilir/bitirebilir (saha gerçekliği:
süpervizör, nöbetçi, vekil). Bu ADR dört kavramı kesin olarak ayırır:

| Kavram                                     | Soru                                   | Nerede                                       |
| ------------------------------------------ | -------------------------------------- | -------------------------------------------- |
| `processes.assignee_user_id`               | Şu an kim sorumlu?                     | tanım satırı — nullable, değişebilir         |
| `process_executions.assignee_user_id`      | Deneme başlarken süreç kime atanmıştı? | yürütme satırı — INSERT'te yazılır, değişmez |
| `started_by`/`completed_by`/`cancelled_by` | İşlemi kim yaptı?                      | mevcut aktör alanları (değişmez)             |
| time-session actor (gelecek)               | Emeği kimin hesabına yazıldı?          | STEP 21D — bağımsız                          |

## "Default assignee" yok — tek seviye

Her `processes` satırı zaten tek bir somut iş kaleminin tanımıdır; "Montaj
hep Harun'a gider" kuralını temsil eden yeniden kullanılabilir bir şablon
domain'i henüz yok. Bu yüzden V1'de `default_assignee_user_id` gibi iki
seviyeli bir model YOKTUR: `assignee_user_id` doğrudan o tanımın mevcut
sorumlusudur. "Daire 12 Montaj → Harun, Daire 13 Montaj → Cihan" zaten
ayrı satırlardır; override diye bir kavram gerekmez.

Gelecekte Process Template/Group domain'i geldiğinde şablon satırında
`default_assignee_user_id` bulunabilir; instantiate sırasında somut
satıra **kopyalanır** (copy-on-instantiate). Şablonun sonradan değişmesi
mevcut süreçleri veya yürütme geçmişini asla geriye dönük yazmaz.

## Kardinalite: 0..1

Bir süreç en fazla bir kullanıcıya atanır (`uuid NULL`). Çoklu atama,
takım atama ve `assignee_type` polimorfizmi bilinçli olarak reddedildi:
gerçek gereksinim olmadan FK bütünlüğü feda edilmez. Takım ataması
geldiğinde yol `assignee_team_id` nullable FK + "tam olarak biri dolu"
CHECK'idir — V1 şeması bunu engellemez.

## Scope güvenliği: composite FK

```sql
FOREIGN KEY (workspace_id, assignee_user_id)
  REFERENCES workspace_memberships (workspace_id, user_id)
```

Bu, farklı workspace/tenant'a ait bir assignee'nin fiziksel olarak
depolanabilmesini imkansız kılar — şemanın üst zincirde kullandığı
composite-FK felsefesinin aynısıdır. MATCH SIMPLE varsayılanı NULL
assignee'yi serbest bırakır.

FK, üyelik **satırının varlığına** bağlanır; güncel uygunluğa değil.
Üyelik iptali soft'dur (`status`/`deleted_at`), satır kalır → depolanmış
assignee tarihsel doğruluğunu korur ("stale"). Uygunluk (aktif kullanıcı +
aktif org üyeliği + aktif workspace üyeliği) uygulama yazılarında
transaction içinde yeniden kanıtlanır ve `FOR SHARE` kilidiyle TOCTOU'a
karşı korunur.

## Stale assignment — kilitli ürün kararı

Üyeliği iptal edilen kullanıcının mevcut atamaları **otomatik temizlenmez**:
150 süreç Harun'da kalır; okuma `eligible=false` bayrağıyla "Artık üye
değil" gösterir; picker ve yeni atamalar Harun'u reddeder. Üye geri
dönerse atamalar kendiliğinden yeniden geçerli olur. `SET NULL`,
bulk-clear veya iptali engelleyen FK davranışı yoktur.

Yürütme snapshot'ı ayrıca `users(id)`'ye FK'lanır — üyelik yaşam
döngüsünden bağımsız olarak kalıcıdır.

## Aktif yürütme altında yeniden atama

Koşan denemenin snapshot'ı korunur; yeniden atama yalnızca gelecekteki
denemelere yansır (transfer workflow'u yok). Yani:

```text
09:00 assignee=Harun  →  09:15 deneme 1 başlar (snapshot=Harun)
10:00 assignee=Cihan  →  deneme 1 hâlâ Harun; deneme 2 başlarsa Cihan
```

Yarış deterministiktir: `assign_process` process satırını `FOR UPDATE`,
`start_execution` aynı satırı `FOR SHARE` okur; ikisi mevcut proje
serileşmesi altında sıralanır. Snapshot her zaman tam bir değerdir —
asla karışık veya eksik değildir.

## RBAC

`processes:assign` — tanım düzenlemeden (`processes:update`) ve yürütme
eylemlerinden (`process_executions:*`) ayrı izin. Sadece sistem Owner'ına
organization scope'ta grant edilir; Member'a verilmez. Self-assign/claim
workflow'u V1'de yoktur.

## API

```text
PUT  .../processes/{process_id}/assignment   {"user_id": uuid | null}
GET  .../workspaces/{workspace_id}/members   {id, display_name}[]
```

PUT idempotent'tir: `null` atamayı kaldırır. Yanıt güncel ProcessPublic'tir
(`assignee: {id, display_name, eligible} | null` gömülü — N+1 yok).
Üye dizini salt-okunurdur ve sıradan workspace erişimiyle açılır — üye
adları operasyonel metadata'dır; yazma yetkisi `processes:assign`'dır.

## Denenen ve reddedilen alternatifler

- `process_assignments` tablosu: 1:0..1 için gereksiz join ve lifecycle.
- Polymorphic `assignee_type`/`assignee_id`: referans bütünlüğünü kırar.
- Assignment event/audit tablosu: genel audit alt sistemi henüz yok;
  mevcut atama + değişmez execution snapshot V1 doğruluğu için yeterlidir.
  Gelecekteki audit fazı ayrıdır.
- `processes:update` ile PATCH-birleşik atama: izin sınırını bulanıklaştırır;
  alan-bazlı yetki karmaşası üretir.

## Erteleme (explicitly out of scope)

Teams, self-claim, assignment history tablosu, Process templates, kişisel
iş kuyruğu sayfası, Active Processes ekranı, zaman oturumları,
bildirim/realtime, transfer-execution komutu, optimistic revision.

## Sınır koşulları

- `processes.assignee_user_id = NULL` geçerlidir; atamasız süreç mevcut
  `process_executions:*` izinleriyle çalıştırılabilir.
- Arşivlenmiş süreç assignment mutation alamaz (route visibility → 404),
  saklı değer tarihsel metadata olarak kalır.
- Execution snapshot'ı hiçbir API yoluyla değiştirilemez (INSERT-time
  server-side derivation; `deny_unknown_fields` client taklidi reddeder).
