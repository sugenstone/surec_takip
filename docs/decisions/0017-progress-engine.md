# Progress Engine (türetilmiş ilerleme motoru)

Tarih: 2026-09-27
Durum: Kabul edildi (STEP 21B)

## Bağlam

STEP 21A (ADR 0016) süreç **yürütme** çekirdeğini verdi: her tanımın değişmez
deneme geçmişi, tek aktif deneme kısıtı ve terminal durumlar. Bu ADR,
tanımlar + geçmişten **gerçek ilerleme** türeten motoru kilitler. İlerleme
Work Item, Section (tüm alt ağaç) ve Project için — liste ve detay
yanıtlarında — tek bir matematik motoruyla hesaplanır.

Tüm kararlar Product Owner tarafından kilitlendi; Phase A tasarımı
onaylandıktan sonra Phase B'de uygulandı.

## Türetilmiş, saklanmaz (derived-not-stored)

Progress hiçbir yerde persist EDİLMEZ: `percent` sütunu, tablosu, trigger'ı,
materialized view veya cache tablosu YOK. Tek doğruluk kaynağı
`processes` + `process_executions` satırlarıdır; her yanıt PostgreSQL'den
set-based bir aggregate ile hesaplanır. Persist edilmiş yüzde reddedildi:
iki gerçek kaynaktan sapar, hiçbir zaman güncel kalamaz ve audit kuralına
aykırıdır.

## Sayılan süreç (counted process)

Bir Process Definition paydaya katılır iff:

```text
p.deleted_at IS NULL AND p.status = 'active'
```

`is_required` V1'de paydayı ETKİLEMEZ — "counted" terminolojisi,
gelecekteki required/dependency semantiğinin public contract'ı bozmadan
evrilmesini sağlar. Sahibinin ulaşılabilirliği de şarttır: work item
görünür (`deleted_at IS NULL AND status <> 'archived'`) ve sahip section
aktif olmalıdır (`deleted_at IS NULL AND status = 'active'`) — route
context'lerinin kuralıyla birebir aynı.

## DONE tanımı

```text
done(p) = EXISTS(completed attempt) AND NOT EXISTS(active attempt)
```

Hiç çalışmamış → done değil; yalnızca iptal → done değil; completed →
done; completed + iptal edilmiş retry → done; **completed + ACTIVE retry →
done DEĞİL**. Aktif rework sırasında ilerlemenin gerilemesi bilinçli ve
doğrudur: progress monoton DEĞİLDİR.

Denemeler asla paydayı etkilemez: bir tanım = bir iş birimi, kaç denemesi
olursa olsun. Sorgular `EXISTS` boolean yüklemleri kullanır; execution
satırları JOIN ile fan-out yapmaz.

## Aggregate contract

```json
{ "completed": 7, "active": 1, "total": 10, "percent": 70 }
```

- `completed`: DONE sağlayan counted tanım sayısı
- `active`: aktif yürütmesi olan counted tanım sayısı
- `total`: tüm counted tanımlar
- `percent`: tamsayı yüzde; **`total = 0` ise `null`** — asla sahte 0
  veya 100 değil

## Yuvarlama — tek otorite

Yüzde yalnızca backend'de hesaplanır (round-half-up, tamsayı aritmetiği:
`(100·completed + total/2) / total`). Kilitli örnekler: 1/3→33, 2/3→67,
1/6→17, 5/6→83, 9/12→75. Frontend gelen değeri verbatim gösterir; asla
ratio yeniden hesaplamaz.

## Leaf-weighted roll-up (ortalama YASAK)

Section/Project ilerlemesi child-card yüzdelerinin ortalaması DEĞİL; tüm
alt ağaçtaki done/total oranıdır:

```text
scope.percent = Σ done counted process (subtree) / Σ counted process (subtree)
```

Section aggregate'i özyinelemeli CTE ile tüm torunları gezer; bir süreç
tek sahip section'a bağlı olduğundan her ataya tam bir kez katkı verir.

## Yapısal değişmezlik (structural invariance)

Sayılan süreç kümesi sabitken; boş wrapper section eklemek, section'ları
yeniden ebeveynlemek Project progress'i DEĞİŞTİREMEZ — aggregate ağaç
şekline değil süreç kümesine bağlıdır. Entegrasyon testiyle kilitlidir.

## Manuel lifecycle bağımsızlığı

Work Item `completed` ve Project `completed` durumları manuel etikettir;
derived progress'i etkilemez ve progress de durumu değiştirmez.
`status=completed` ile `percent=80` geçerli bir kombinasyondur.

## Tek ifade snapshot kuralı

Her aggregate'de `completed`, `active`, `total`, `percent` TEK SQL
ifadesinden gelir — pay ve payda asla farklı snapshot'lardan karışamaz.
Eşzamanlı geçişler eski veya yeni geçerli snapshot üretebilir; imkânsız
karma sonuç (`completed > total`) üretemez.

## Liste yanıtları — N+1 yasak

Liste satırı başına ayrı sorgu YOK:

- Work item listesi: `GROUP BY work_item_id` ile tek statement.
- Section listesi: multi-root recursive CTE `(root, node)` çiftleri taşır
  ve `GROUP BY root` — her child kendi alt ağacının aggregate'ini alır.
- Project listesi: `GROUP BY project_id` tek statement.

## Migration 012

Yalnız destekleyici performans indexi:

```sql
CREATE INDEX process_executions_completed_idx
  ON process_executions (process_id) WHERE status = 'completed';
```

`DONE` yükleminin "hiç completed deneme var mı?" EXISTS probu için;
aktif-deneme probu zaten `process_executions_one_active` (011) ile
kaplıdır.

## API yerleşimi

`progress` alanı mevcut `WorkItemPublic`, `SectionPublic`,
`ProjectPublic` gövdelerine gömülüdür; ayrı `/progress` endpoint'i YOK.
Liste ve detay yanıtları aynı şemayı taşır. Sıfır-iş kapsamında UI sahte
bar yerine "—" ve lokalize açıklama gösterir.

## Güvenlik

Her sorgu tenant + workspace + project (+ section/work-item) zinciriyle
bağlanır; yabancı tenant veya parent satırları aggregate'e giremez.
Yanlış parent zinciri progress=0 DEĞİL mevcut 404 semantiğini döndürür —
varlık sızıntısı yoktur.

## Gelecek uyumluluk

`is_required`, bağımlılık/blokaj, assignment, time sessions, Active
Processes ve TV yüzeyleri bu contract'ı genişletebilir; `counted`/`done`
tanımları değişmez çekirdek kalır.
