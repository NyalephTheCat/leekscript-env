# Appendix F — Builtin and global signatures catalog

**Informative.** This appendix is **generated** from **bundled signature-definition sources**: a **stdlib-oriented** layer and a **game-host API** layer (Leek-typed **`global`** and **`function`** headers). **Runtime** arity and behavior **MUST** still match the **interpreter** and **VM export parity** tests.

Regenerate via **`python3 scripts/gen_spec_appendices.py`** from the repository root.

## Stdlib-oriented signatures

**Globals** (20 rows)

| Global | Type |
|--------|------|
| `E` | `real` |
| `Infinity` | `real` |
| `NaN` | `real` |
| `PI` | `real` |
| `SORT_ASC` | `integer` |
| `SORT_DESC` | `integer` |
| `TYPE_ARRAY` | `integer` |
| `TYPE_BOOLEAN` | `integer` |
| `TYPE_CLASS` | `integer` |
| `TYPE_FUNCTION` | `integer` |
| `TYPE_INTERVAL` | `integer` |
| `TYPE_MAP` | `integer` |
| `TYPE_NULL` | `integer` |
| `TYPE_NUMBER` | `integer` |
| `TYPE_OBJECT` | `integer` |
| `TYPE_SET` | `integer` |
| `TYPE_STRING` | `integer` |
| `COLOR_BLUE` | `integer` |
| `COLOR_GREEN` | `integer` |
| `COLOR_RED` | `integer` |


**Functions** (170 rows)

| Signature |
|-------------|
| `abs(integer|real a) => integer|real` |
| `acos(integer|real a) => real` |
| `asin(integer|real a) => real` |
| `atan(integer|real a) => real` |
| `atan2(integer|real a, integer|real b) => real` |
| `binString(integer a) => string` |
| `bitCount(integer a) => integer` |
| `bitReverse(integer a) => integer` |
| `bitsToReal(integer a) => real` |
| `byteReverse(integer a) => integer` |
| `cbrt(integer|real a) => real` |
| `ceil(integer|real a) => integer` |
| `cos(integer|real a) => real` |
| `exp(integer|real a) => real` |
| `floor(integer|real a) => integer` |
| `hexString(integer a) => string` |
| `hypot(integer|real a, integer|real b) => real` |
| `isFinite(real a) => boolean` |
| `isInfinite(real a) => boolean` |
| `isNaN(real a) => boolean` |
| `isPermutation(integer a, integer b) => boolean` |
| `leadingZeros(integer a) => integer` |
| `log(integer|real a) => real` |
| `log10(integer|real a) => real` |
| `log2(integer|real a) => real` |
| `max(integer|real a, integer|real b) => integer|real` |
| `min(integer|real a, integer|real b) => integer|real` |
| `number(string|integer|real a) => integer|real` |
| `pow(integer|real base, integer|real exponent) => integer|real` |
| `rand() => real` |
| `randFloat(real a, real b) => real` |
| `randInt(integer a, integer b) => integer` |
| `randReal(real a, real b) => real` |
| `realBits(real a) => integer` |
| `rotateLeft(integer a, integer b) => integer` |
| `rotateRight(integer a, integer b) => integer` |
| `round(real a) => integer` |
| `signum(integer|real a) => integer` |
| `sin(integer|real a) => real` |
| `sqrt(integer|real a) => real` |
| `tan(integer|real a) => real` |
| `toDegrees(real a) => real` |
| `toRadians(real a) => real` |
| `trailingZeros(integer a) => integer` |
| `charAt(string str, integer i) => string` |
| `codePointAt(string str, integer i) => integer` |
| `contains(string str, string search) => boolean` |
| `endsWith(string str, string suffix) => boolean` |
| `indexOf(string str, string search) => integer` |
| `length(string str) => integer` |
| `replace(string str, string oldChar, string newChar) => string` |
| `split(string str, string sep) => Array<string>` |
| `split(string str, string sep, number limit) => Array<string>` |
| `startsWith(string str, string prefix) => boolean` |
| `string(any value) => string` |
| `substring(string str, integer start, integer length) => string` |
| `subString(string str, integer start) => string` |
| `toLower(string str) => string` |
| `toUpper(string str) => string` |
| `arrayChunk<T>(Array<T> array, integer chunkSize) => Array<Array<T>>` |
| `arrayConcat<T>(Array<T> array1, Array<T> array2) => Array<T>` |
| `arrayClear<T>(Array<T> array) => void` |
| `arrayEvery<T>(Array<T> array, Function<T => boolean> predicate) => boolean` |
| `arrayFilter<T>(Array<T> array, Function<T => boolean> predicate) => Array<T>` |
| `arrayFlatten<T>(Array<T> array, integer depth = 1) => Array<T>` |
| `arrayFoldLeft<T, U>(Array<T> array, Function<T, U => U>|Function<T, U, integer => U>|Function<T, U, integer, Array<T> => U> function, U accumulator) => T` |
| `arrayFoldRight<T, U>(Array<T> array, Function<T, U => U>|Function<T, U, integer => U>|Function<T, U, integer, Array<T> => U> function, U accumulator) => T` |
| `arrayFrequencies<T>(Array<T> array) => Map<T, integer>` |
| `arrayGet<T>(Array<T> array, integer i, T defaultValue) => T` |
| `arrayGetOrElse<T>(Array<T> array, integer i) => T?` |
| `arrayIter<T>(Array<T> array, Function<T => void>|Function<T, integer => void>|Function<T, integer, Array<T> => void> callback) => Array<T>` |
| `arrayMap<T, U>(Array<T> array, Function<T => U>|Function<T, integer => U>|Function<T, integer, Array<T> => U> callback) => Array<U>` |
| `arrayMax<T>(Array<T> array) => T` |
| `arrayMin<T>(Array<T> array) => T` |
| `arrayPartition<T>(Array<T> array, Function<T => boolean>|Function<T, integer => boolean>|Function<T, integer, Array<T> => boolean> predicate) => Array<Array<T>>` |
| `arrayRandom<T>(Array<T> array, integer n) => Array<T>` |
| `arrayRemove<T>(Array<T> array, T value) => Array<T>` |
| `arraySlice<T>(Array<T> array, integer start, integer end = count(array), integer step = 1) => Array<T>` |
| `arraySome<T>(Array<T> array, Function<T => boolean>|Function<T, integer => boolean>|Function<T, integer, Array<T> => boolean> predicate) => boolean` |
| `arraySort<T>(Array<T> array, Function<T, T => integer>|Function<T, T, integer => integer>|Function<T, T, integer, Array<T> => integer> comparator) => Array<T>` |
| `arrayToSet<T>(Array<T> array) => Set<T>` |
| `arrayUnique<T>(Array<T> array) => Array<T>` |
| `average(Array<integer|real> array) => real` |
| `count(Array<any> array) => integer` |
| `inArray<T>(Array<T> array, T element) => boolean` |
| `isEmpty(Array<any> array) => boolean` |
| `join(Array array, string separator) => string` |
| `pop<T>(Array<T> array) => T` |
| `remove<T>(Array<T> array, integer i) => T` |
| `indexOf<T>(Array<T> array, T element, integer start = 0) => integer` |
| `shift<T>(Array<T> array) => T` |
| `subArray<T>(Array<T> array, integer start, integer end) => Array<T>` |
| `sum(Array<integer> array) => integer` |
| `sum(Array<integer|real> array) => real` |
| `resize<T>(Array<T> array, T value, integer size = count(array)) => Array<T>` |
| `push<T>(Array<T> array, T element) => Array<T>` |
| `remove<T>(Array<T> array, T value) => Array<T>` |
| `reverse<T>(Array<T> array) => Array<T>` |
| `shuffle<T>(Array<T> array) => void` |
| `sort<T>(Array<T> array, integer order = 0) => Array<T>` |
| `unshift<T>(Array<T> array, T element) => void` |
| `mapAverage<K>(Map<K, integer|real> map) => real` |
| `mapContains<K, V>(Map<K, V> map, V value) => boolean` |
| `mapContainsKey<K, V>(Map<K, V> map, K key) => boolean` |
| `mapEvery<K, V>(Map<K, V> map, Function<V => boolean>|Function<V, K => boolean>|Function<V, K, Map<K, V> => boolean> predicate) => boolean` |
| `mapFilter<K, V>(Map<K, V> map, Function<V => boolean>|Function<V, K => boolean>|Function<V, K, Map<K, V> => boolean> predicate) => Map<K, V>` |
| `mapFold<K, V, U>(Map<K, V> map, Function<U, V => U>|Function<U, V, K => U>|Function<U, V, K, Map<K, V> => U> reducer, U accumulator) => U` |
| `mapGet<K, V>(Map<K, V> map, K key, V defaultValue) => V` |
| `mapIsEmpty<K, V>(Map<K, V> map) => boolean` |
| `mapKeys<K, V>(Map<K, V> map) => Array<K>` |
| `mapMap<K, V, U>(Map<K, V> map, Function<V => U>|Function<V, K => U>|Function<V, K, Map<K, V> => U> callback) => Map<K, U>` |
| `mapMax<T, U>(Map<T, U> map) => U?` |
| `mapMerge<T, U>(Map<T, U> map1, Map<T, U> map2) => Map<T, U>` |
| `mapMin<T, U>(Map<T, U> map) => U?` |
| `mapPut<T, U>(Map<T, U> map, T key, U value) => Map<T, U>` |
| `mapRemove<T, U>(Map<T, U> map, T key) => U` |
| `mapReplace<T, U>(Map<T, U> map, T key, U value) => U?` |
| `mapReplaceAll<T, U>(Map<T, U> map, Map<T, U> map2) => Map<T, U>` |
| `mapSearch<T, U>(Map<T, U> map, U value) => T?` |
| `mapSize<T, U>(Map<T, U> map) => integer` |
| `mapSome<K, V>(Map<K, V> map, Function<V => boolean>|Function<V, K => boolean>|Function<V, K, Map<K, V> => boolean> predicate) => boolean` |
| `mapSum<T>(Map<T, integer> map) => integer` |
| `mapSum<T>(Map<T, integer|real> map) => real` |
| `mapValues<T, U>(Map<T, U> map) => Array<U>` |
| `mapClear<T, U>(Map<T, U> map) => void` |
| `mapFill<T, U>(Map<T, U> map, U value) => void` |
| `mapIter<T, U>(Map<T, U> map, Function<U => void>|Function<T, U => void>|Function<T, U, Map<T, U> => void> callback) => void` |
| `mapReplaceAll<T, U>(Map<T, U> map, Map<T, U> map2) => Map<T, U>` |
| `mapRemoveAll<T, U>(Map<T, U> map, U value) => Map<T, U>` |
| `clone<T>(T value) => T` |
| `clone<T>(T value, integer level) => T` |
| `debug(any message) => void` |
| `debugC(any message, integer color) => void` |
| `debugE(any message) => void` |
| `debugW(any message) => void` |
| `jsonDecode(string str) => any` |
| `jsonEncode(any value) => string` |
| `typeOf(any value) => integer` |
| `getBlue(integer color) => integer` |
| `getGreen(integer color) => integer` |
| `getRed(integer color) => integer` |
| `getColor(integer blue, integer green, integer red) => integer` |
| `setClear(Set<any> set) => void` |
| `setContains<T>(Set<T> set, T value) => boolean` |
| `setDifference<T>(Set<T> setA, Set<T> setB) => Set<T>` |
| `setDisjunction<T>(Set<T> setA, Set<T> setB) => Set<T>` |
| `setIntersection<T>(Set<T> setA, Set<T> setB) => Set<T>` |
| `setIsEmpty<T>(Set<T> set) => boolean` |
| `setIsSubsetOf<T>(Set<T> setA, Set<T> setB) => boolean` |
| `setInsert<T>(Set<T> set, T value) => boolean` |
| `setRemove<T>(Set<T> set, T value) => boolean` |
| `setSize<T>(Set<T> set) => integer` |
| `setPut<T>(Set<T> set, T value) => boolean` |
| `setToArray<T>(Set<T> set) => Array<T>` |
| `setUnion<T>(Set<T> setA, Set<T> setB) => Set<T>` |
| `intervalAverage(Interval interval) => real` |
| `intervalCombine<T>(Interval<T> intervalA, Interval<T> intervalB) => Interval<T>` |
| `intervalIntersection<T>(Interval<T> intervalA, Interval<T> intervalB) => Interval<T>` |
| `intervalIsBounded(Interval interval) => boolean` |
| `intervalIsClosed(Interval interval) => boolean` |
| `intervalIsEmpty(Interval interval) => boolean` |
| `intervalIsLeftBounded(Interval interval) => boolean` |
| `intervalIsRightBounded(Interval interval) => boolean` |
| `intervalIsLeftClosed(Interval interval) => boolean` |
| `intervalIsRightClosed(Interval interval) => boolean` |
| `intervalMax<T>(Interval<T> interval) => T` |
| `intervalMin<T>(Interval<T> interval) => T` |
| `intervalSize<T>(Interval<T> interval) => T` |
| `intervalValues<T>(Interval<T> interval, T step = 1) => Array<T>` |
| `intervalToSet<T>(Interval<T> interval) => Set<T>` |


## Game-host API signatures

**Globals** (345 rows)

| Global | Type |
|--------|------|
| `BULB_FIRE` | `integer` |
| `BULB_HEALER` | `integer` |
| `BULB_ICED` | `integer` |
| `BULB_LIGHTNING` | `integer` |
| `BULB_METALLIC` | `integer` |
| `BULB_PUNY` | `integer` |
| `BULB_ROCKY` | `integer` |
| `BULB_SAVANT` | `integer` |
| `BULB_TACTICIAN` | `integer` |
| `BULB_WIZARD` | `integer` |
| `EFFECT_ABSOLUTE_SHIELD` | `integer` |
| `EFFECT_ABSOLUTE_VULNERABILITY` | `integer` |
| `EFFECT_ADD_STATE` | `integer` |
| `EFFECT_AFTEREFFECT` | `integer` |
| `EFFECT_ALLY_KILLED_TO_AGILITY` | `integer` |
| `EFFECT_ANTIDOTE` | `integer` |
| `EFFECT_ATTRACT` | `integer` |
| `EFFECT_BOOST_MAX_LIFE` | `integer` |
| `EFFECT_BUFF_AGILITY` | `integer` |
| `EFFECT_BUFF_FORCE` | `integer` |
| `EFFECT_BUFF_MP` | `integer` |
| `EFFECT_BUFF_RESISTANCE` | `integer` |
| `EFFECT_BUFF_STRENGTH` | `integer` |
| `EFFECT_BUFF_TP` | `integer` |
| `EFFECT_BUFF_WISDOM` | `integer` |
| `EFFECT_CRITICAL_TO_HEAL` | `integer` |
| `EFFECT_DAMAGE` | `integer` |
| `EFFECT_DAMAGE_RETURN` | `integer` |
| `EFFECT_DAMAGE_TO_ABSOLUTE_SHIELD` | `integer` |
| `EFFECT_DAMAGE_TO_STRENGTH` | `integer` |
| `EFFECT_DEBUFF` | `integer` |
| `EFFECT_HEAL` | `integer` |
| `EFFECT_INVERT` | `integer` |
| `EFFECT_KILL` | `integer` |
| `EFFECT_KILL_TO_TP` | `integer` |
| `EFFECT_LIFE_DAMAGE` | `integer` |
| `EFFECT_MODIFIER_IRREDUCTIBLE` | `integer` |
| `EFFECT_MODIFIER_MULTIPLIED_BY_TARGETS` | `integer` |
| `EFFECT_MODIFIER_NOT_REPLACEABLE` | `integer` |
| `EFFECT_MODIFIER_ON_CASTER` | `integer` |
| `EFFECT_MODIFIER_STACKABLE` | `integer` |
| `EFFECT_MOVED_TO_MP` | `integer` |
| `EFFECT_NOVA_DAMAGE` | `integer` |
| `EFFECT_NOVA_DAMAGE_TO_MAGIC` | `integer` |
| `EFFECT_NOVA_VITALITY` | `integer` |
| `EFFECT_POISON` | `integer` |
| `EFFECT_POISON_TO_SCIENCE` | `integer` |
| `EFFECT_PROPAGATION` | `integer` |
| `EFFECT_PUSH` | `integer` |
| `EFFECT_RAW_ABSOLUTE_SHIELD` | `integer` |
| `EFFECT_RAW_BUFF_AGILITY` | `integer` |
| `EFFECT_RAW_BUFF_MAGIC` | `integer` |
| `EFFECT_RAW_BUFF_MP` | `integer` |
| `EFFECT_RAW_BUFF_POWER` | `integer` |
| `EFFECT_RAW_BUFF_RESISTANCE` | `integer` |
| `EFFECT_RAW_BUFF_SCIENCE` | `integer` |
| `EFFECT_RAW_BUFF_STRENGTH` | `integer` |
| `EFFECT_RAW_BUFF_TP` | `integer` |
| `EFFECT_RAW_BUFF_WISDOM` | `integer` |
| `EFFECT_RAW_HEAL` | `integer` |
| `EFFECT_RAW_RELATIVE_SHIELD` | `integer` |
| `EFFECT_RELATIVE_SHIELD` | `integer` |
| `EFFECT_REMOVE_SHACKLES` | `integer` |
| `EFFECT_REPEL` | `integer` |
| `EFFECT_RESURRECT` | `integer` |
| `EFFECT_SHACKLE_AGILITY` | `integer` |
| `EFFECT_SHACKLE_MAGIC` | `integer` |
| `EFFECT_SHACKLE_MP` | `integer` |
| `EFFECT_SHACKLE_STRENGTH` | `integer` |
| `EFFECT_SHACKLE_TP` | `integer` |
| `EFFECT_SHACKLE_WISDOM` | `integer` |
| `EFFECT_SLIDE_TO` | `integer` |
| `EFFECT_STEAL_ABSOLUTE_SHIELD` | `integer` |
| `EFFECT_SUMMON` | `integer` |
| `EFFECT_TARGET_ALLIES` | `integer` |
| `EFFECT_TARGET_ALWAYS_CASTER` | `integer` |
| `EFFECT_TARGET_CASTER` | `integer` |
| `EFFECT_TARGET_ENEMIES` | `integer` |
| `EFFECT_TARGET_NON_SUMMONS` | `integer` |
| `EFFECT_TARGET_NOT_CASTER` | `integer` |
| `EFFECT_TARGET_SUMMONS` | `integer` |
| `EFFECT_TELEPORT` | `integer` |
| `EFFECT_VULNERABILITY` | `integer` |
| `ENTITY_BULB` | `integer` |
| `ENTITY_CHEST` | `integer` |
| `ENTITY_LEEK` | `integer` |
| `ENTITY_MOB` | `integer` |
| `ENTITY_TURRET` | `integer` |
| `MOB_BLUE_CRYSTAL` | `integer` |
| `MOB_EVIL_PUMPKIN` | `integer` |
| `MOB_FENNEL_KING` | `integer` |
| `MOB_FENNEL_KNIGHT` | `integer` |
| `MOB_FENNEL_SCRIBE` | `integer` |
| `MOB_FENNEL_SQUIRE` | `integer` |
| `MOB_GRAAL` | `integer` |
| `MOB_GREEN_CRYSTAL` | `integer` |
| `MOB_HUBBARD` | `integer` |
| `MOB_NASU_RONIN` | `integer` |
| `MOB_NASU_SAMURAI` | `integer` |
| `MOB_NASU_SEITO` | `integer` |
| `MOB_NASU_WARRIOR` | `integer` |
| `MOB_OFFSPRING` | `integer` |
| `MOB_RED_CRYSTAL` | `integer` |
| `MOB_TURBAN` | `integer` |
| `MOB_WARTY` | `integer` |
| `MOB_YELLOW_CRYSTAL` | `integer` |
| `STATE_INVINCIBLE` | `integer` |
| `STAT_ABSOLUTE_SHIELD` | `integer` |
| `STAT_AGILITY` | `integer` |
| `STAT_CORES` | `integer` |
| `STAT_DAMAGE_RETURN` | `integer` |
| `STAT_FREQUENCY` | `integer` |
| `STAT_LIFE` | `integer` |
| `STAT_MAGIC` | `integer` |
| `STAT_MP` | `integer` |
| `STAT_POWER` | `integer` |
| `STAT_RAM` | `integer` |
| `STAT_RELATIVE_SHIELD` | `integer` |
| `STAT_RESISTANCE` | `integer` |
| `STAT_SCIENCE` | `integer` |
| `STAT_STRENGTH` | `integer` |
| `STAT_TP` | `integer` |
| `STAT_WISDOM` | `integer` |
| `USE_CRITICAL` | `integer` |
| `USE_FAILED` | `integer` |
| `USE_INVALID_COOLDOWN` | `integer` |
| `USE_INVALID_POSITION` | `integer` |
| `USE_INVALID_TARGET` | `integer` |
| `USE_MAX_USES` | `integer` |
| `USE_NOT_ENOUGH_TP` | `integer` |
| `USE_SUCCESS` | `integer` |
| `USE_TOO_MANY_SUMMONS` | `integer` |
| `EFFECT_STEAL_LIFE` | `integer` |
| `EFFECT_TOTAL_DEBUFF` | `integer` |
| `WEAPON_AXE` | `integer` |
| `WEAPON_BAZOOKA` | `integer` |
| `WEAPON_BROADSWORD` | `integer` |
| `WEAPON_B_LASER` | `integer` |
| `WEAPON_DARK_KATANA` | `integer` |
| `WEAPON_DESTROYER` | `integer` |
| `WEAPON_DOUBLE_GUN` | `integer` |
| `WEAPON_ELECTRISOR` | `integer` |
| `WEAPON_ENHANCED_LIGHTNINGER` | `integer` |
| `WEAPON_EXCALIBUR` | `integer` |
| `WEAPON_EXPLORER_RIFLE` | `integer` |
| `WEAPON_FLAME_THROWER` | `integer` |
| `WEAPON_GAZOR` | `integer` |
| `WEAPON_GRENADE_LAUNCHER` | `integer` |
| `WEAPON_HEAVY_SWORD` | `integer` |
| `WEAPON_ILLICIT_GRENADE_LAUNCHER` | `integer` |
| `WEAPON_J_LASER` | `integer` |
| `WEAPON_KATANA` | `integer` |
| `WEAPON_LASER` | `integer` |
| `WEAPON_LIGHTNINGER` | `integer` |
| `WEAPON_MACHINE_GUN` | `integer` |
| `WEAPON_MAGNUM` | `integer` |
| `WEAPON_MYSTERIOUS_ELECTRISOR` | `integer` |
| `WEAPON_M_LASER` | `integer` |
| `WEAPON_NEUTRINO` | `integer` |
| `WEAPON_ODACHI` | `integer` |
| `WEAPON_PISTOL` | `integer` |
| `WEAPON_QUANTUM_RIFLE` | `integer` |
| `WEAPON_REVOKED_M_LASER` | `integer` |
| `WEAPON_RHINO` | `integer` |
| `WEAPON_RIFLE` | `integer` |
| `WEAPON_SCYTHE` | `integer` |
| `WEAPON_SHOTGUN` | `integer` |
| `WEAPON_SWORD` | `integer` |
| `WEAPON_UNBRIDLED_GAZOR` | `integer` |
| `WEAPON_UNSTABLE_DESTROYER` | `integer` |
| `CHIP_ACCELERATION` | `integer` |
| `CHIP_ADRENALINE` | `integer` |
| `CHIP_ALTERATION` | `integer` |
| `CHIP_ANTIDOTE` | `integer` |
| `CHIP_APOCALYPSE` | `integer` |
| `CHIP_ARMOR` | `integer` |
| `CHIP_ARMORING` | `integer` |
| `CHIP_ARSENIC` | `integer` |
| `CHIP_AWEKENING` | `integer` |
| `CHIP_BALL_AND_CHAIN` | `integer` |
| `CHIP_BANDAGE` | `integer` |
| `CHIP_BARK` | `integer` |
| `CHIP_BOXING_GLOVE` | `integer` |
| `CHIP_BRAINWASHING` | `integer` |
| `CHIP_BRAMBLE` | `integer` |
| `CHIP_BURNING` | `integer` |
| `CHIP_CARAPACE` | `integer` |
| `CHIP_COLLAR` | `integer` |
| `CHIP_COVETOUSNESS` | `integer` |
| `CHIP_COVID` | `integer` |
| `CHIP_CRUSHING` | `integer` |
| `CHIP_CURE` | `integer` |
| `CHIP_DESINTEGRATION` | `integer` |
| `CHIP_DEVIL_STRIKE` | `integer` |
| `CHIP_DIVINE_PROTECTION` | `integer` |
| `CHIP_DOME` | `integer` |
| `CHIP_DOPING` | `integer` |
| `CHIP_DRIP` | `integer` |
| `CHIP_ELEVATION` | `integer` |
| `CHIP_FEROCITY` | `integer` |
| `CHIP_FERTILIZER` | `integer` |
| `CHIP_FIRE_BULB` | `integer` |
| `CHIP_FLAME` | `integer` |
| `CHIP_FLASH` | `integer` |
| `CHIP_FORTRESS` | `integer` |
| `CHIP_FRACTURE` | `integer` |
| `CHIP_GRAPPLE` | `integer` |
| `CHIP_HEALER_BULB` | `integer` |
| `CHIP_HELMET` | `integer` |
| `CHIP_ICE` | `integer` |
| `CHIP_ICEBERG` | `integer` |
| `CHIP_ICED_BULB` | `integer` |
| `CHIP_INVERSION` | `integer` |
| `CHIP_JUMP` | `integer` |
| `CHIP_KILL` | `integer` |
| `CHIP_KNOWLEDGE` | `integer` |
| `CHIP_LEATHER_BOOTS` | `integer` |
| `CHIP_LIBERATION` | `integer` |
| `CHIP_LIGHTNING` | `integer` |
| `CHIP_LIGHTNING_BULB` | `integer` |
| `CHIP_LOAM` | `integer` |
| `CHIP_MANUMISSION` | `integer` |
| `CHIP_METALLIC_BULB` | `integer` |
| `CHIP_METEORITE` | `integer` |
| `CHIP_MIRROR` | `integer` |
| `CHIP_MOTIVATION` | `integer` |
| `CHIP_MUTATION` | `integer` |
| `CHIP_PEBBLE` | `integer` |
| `CHIP_PLAGUE` | `integer` |
| `CHIP_PLASMA` | `integer` |
| `CHIP_PRECIPITATION` | `integer` |
| `CHIP_PRISM` | `integer` |
| `CHIP_PROTEIN` | `integer` |
| `CHIP_PUNISHMENT` | `integer` |
| `CHIP_PUNY_BULB` | `integer` |
| `CHIP_RAGE` | `integer` |
| `CHIP_RAMPART` | `integer` |
| `CHIP_REFLEXES` | `integer` |
| `CHIP_REGENERATION` | `integer` |
| `CHIP_REMISSION` | `integer` |
| `CHIP_REPOTTING` | `integer` |
| `CHIP_RESURRECTION` | `integer` |
| `CHIP_ROCK` | `integer` |
| `CHIP_ROCKFALL` | `integer` |
| `CHIP_ROCKY_BULB` | `integer` |
| `CHIP_SAVANT_BULB` | `integer` |
| `CHIP_SERUM` | `integer` |
| `CHIP_SEVEN_LEAGUE_BOOTS` | `integer` |
| `CHIP_SHIELD` | `integer` |
| `CHIP_SHOCK` | `integer` |
| `CHIP_SLOW_DOWN` | `integer` |
| `CHIP_SOLIDIFICATION` | `integer` |
| `CHIP_SOPORIFIC` | `integer` |
| `CHIP_SPARK` | `integer` |
| `CHIP_STALACTITE` | `integer` |
| `CHIP_STEROID` | `integer` |
| `CHIP_STRETCHING` | `integer` |
| `CHIP_TACTICIAN_BULB` | `integer` |
| `CHIP_TELEPORTATION` | `integer` |
| `CHIP_THERAPY` | `integer` |
| `CHIP_THORN` | `integer` |
| `CHIP_TOXIN` | `integer` |
| `CHIP_TRANQUILIZER` | `integer` |
| `CHIP_TRANSMUTATION` | `integer` |
| `CHIP_VACCINE` | `integer` |
| `CHIP_VAMPIRIZATION` | `integer` |
| `CHIP_VENOM` | `integer` |
| `CHIP_WALL` | `integer` |
| `CHIP_WARM_UP` | `integer` |
| `CHIP_WHIP` | `integer` |
| `CHIP_WINGED_BOOTS` | `integer` |
| `CHIP_WIZARDRY` | `integer` |
| `CHIP_WIZARD_BULB` | `integer` |
| `USE_RESURRECT_INVALID_ENTITY` | `integer` |
| `CELL_EMPTY` | `integer` |
| `CELL_ENTITY` | `integer` |
| `CELL_OBSTACLE` | `integer` |
| `CELL_PLAYER` | `integer` |
| `CRITICAL_FACTOR` | `real` |
| `MAP_BEACH` | `integer` |
| `MAP_CASTLE` | `integer` |
| `MAP_CEMETERY` | `integer` |
| `MAP_DESERT` | `integer` |
| `MAP_FACTORY` | `integer` |
| `MAP_FOREST` | `integer` |
| `MAP_GLACIER` | `integer` |
| `MAP_NEXUS` | `integer` |
| `MAP_TEIEN` | `integer` |
| `MAP_TEMPLE` | `integer` |
| `AREA_ALLIES` | `integer` |
| `AREA_CIRCLE_1` | `integer` |
| `AREA_CIRCLE_2` | `integer` |
| `AREA_CIRCLE_3` | `integer` |
| `AREA_ENEMIES` | `integer` |
| `AREA_FIRST_INLINE` | `integer` |
| `AREA_LASER_LINE` | `integer` |
| `AREA_PLUS_1` | `integer` |
| `AREA_PLUS_2` | `integer` |
| `AREA_PLUS_3` | `integer` |
| `AREA_POINT` | `integer` |
| `AREA_SQUARE_1` | `integer` |
| `AREA_SQUARE_2` | `integer` |
| `AREA_X_1` | `integer` |
| `AREA_X_2` | `integer` |
| `AREA_X_3` | `integer` |
| `BOSS_EVIL_PUMPKIN` | `integer` |
| `BOSS_FENNEL_KING` | `integer` |
| `BOSS_NASU_SAMURAI` | `integer` |
| `FIGHT_CONTEXT_BATTLE_ROYALE` | `integer` |
| `FIGHT_CONTEXT_CHALLENGE` | `integer` |
| `FIGHT_CONTEXT_GARDEN` | `integer` |
| `FIGHT_CONTEXT_TEST` | `integer` |
| `FIGHT_CONTEXT_TOURNAMENT` | `integer` |
| `FIGHT_TYPE_BATTLE_ROYALE` | `integer` |
| `FIGHT_TYPE_BOSS` | `integer` |
| `FIGHT_TYPE_CHEST_HUNT` | `integer` |
| `FIGHT_TYPE_COLOSSUS` | `integer` |
| `FIGHT_TYPE_FARMER` | `integer` |
| `FIGHT_TYPE_SOLO` | `integer` |
| `FIGHT_TYPE_TEAM` | `integer` |
| `FIGHT_TYPE_WAR` | `integer` |
| `LAUNCH_TYPE_CIRCLE` | `integer` |
| `LAUNCH_TYPE_DIAGONAL` | `integer` |
| `LAUNCH_TYPE_DIAGONAL_INVERTED` | `integer` |
| `LAUNCH_TYPE_LINE` | `integer` |
| `LAUNCH_TYPE_LINE_INVERTED` | `integer` |
| `LAUNCH_TYPE_STAR` | `integer` |
| `LAUNCH_TYPE_STAR_INVERTED` | `integer` |
| `MAX_TURNS` | `integer` |
| `SUMMON_LIMIT` | `integer` |
| `INSTRUCTIONS_LIMIT` | `integer` |
| `OPERATIONS_LIMIT` | `integer` |
| `MESSAGE_ATTACK` | `integer` |
| `MESSAGE_BUFF_AGILITY` | `integer` |
| `MESSAGE_BUFF_FORCE` | `integer` |
| `MESSAGE_BUFF_MP` | `integer` |
| `MESSAGE_BUFF_TP` | `integer` |
| `MESSAGE_CUSTOM` | `integer` |
| `MESSAGE_DEBUFF` | `integer` |
| `MESSAGE_HEAL` | `integer` |
| `MESSAGE_MOVE_AWAY` | `integer` |
| `MESSAGE_MOVE_AWAY_CELL` | `integer` |
| `MESSAGE_MOVE_TOWARD` | `integer` |
| `MESSAGE_MOVE_TOWARD_CELL` | `integer` |
| `MESSAGE_SHIELD` | `integer` |


**Functions** (203 rows)

| Signature |
|-------------|
| `getAbsoluteShield(integer entity = getEntity()) => integer` |
| `getAgility(integer entity = getEntity()) => integer` |
| `getAIId(integer entity = getEntity()) => integer` |
| `getAIName(integer entity = getEntity()) => string` |
| `getBirthTurn(integer entity = getEntity()) => integer` |
| `getCell(integer entity = getEntity()) => integer` |
| `getChips(integer entity = getEntity()) => Array<integer>` |
| `getCores(integer entity = getEntity()) => integer` |
| `getDamageReturn(integer entity = getEntity()) => integer` |
| `getEffects(integer entity = getEntity()) => Array<Array<integer|boolean>>` |
| `getEntity() => integer` |
| `getEntityTurnOrder(integer entity = getEntity()) => integer` |
| `getFarmerCountry(integer entity = getEntity()) => string` |
| `getFarmerId(integer entity = getEntity()) => integer` |
| `getFarmerName(integer entity = getEntity()) => string` |
| `getForce(integer entity = getEntity()) => integer` |
| `getFrequency(integer entity = getEntity()) => integer` |
| `getItemMaxUses(integer item) => integer` |
| `getLaunchedEffects(integer entity = getEntity()) => Array<Array<integer|boolean>>` |
| `getLeek() => integer` |
| `getLeekID(integer entity = getEntity()) => integer?` |
| `getLevel(integer entity = getEntity()) => integer` |
| `getLife(integer entity = getEntity()) => integer` |
| `getMagic(integer entity = getEntity()) => integer` |
| `getMP(integer entity = getEntity()) => integer` |
| `getMobType(integer entity = getEntity()) => integer` |
| `getName(integer entity = getEntity()) => string` |
| `getPassiveEffects(integer entity = getEntity()) => Array<Array<integer|boolean>>` |
| `getPower(integer entity = getEntity()) => integer` |
| `getRAM(integer entity = getEntity()) => integer` |
| `getRelativeShield(integer entity = getEntity()) => integer` |
| `getResistance(integer entity = getEntity()) => integer` |
| `getScience(integer entity = getEntity()) => integer` |
| `getSide(integer entity = getEntity()) => integer` |
| `getStates(integer entity = getEntity()) => Set<integer>` |
| `getStat(integer entity = getEntity(), integer stat) => integer` |
| `getStrength(integer entity = getEntity()) => integer` |
| `getSummoner(integer entity = getEntity()) => integer?` |
| `getSummons(integer entity = getEntity()) => Array<integer>` |
| `getTeamID(integer entity = getEntity()) => integer` |
| `getTeamName(integer entity = getEntity()) => string` |
| `getTotalLife(integer entity = getEntity()) => integer` |
| `getTotalMP(integer entity = getEntity()) => integer` |
| `getTotalTP(integer entity = getEntity()) => integer` |
| `getTP(integer entity = getEntity()) => integer` |
| `getType(integer entity = getEntity()) => integer` |
| `getWeapon(integer entity = getEntity()) => integer?` |
| `getWeapons(integer entity = getEntity()) => Array<integer>` |
| `getWisdom(integer entity = getEntity()) => integer` |
| `isAlive(integer entity = getEntity()) => boolean` |
| `isAlly(integer entity = getEntity()) => boolean` |
| `isDead(integer entity = getEntity()) => boolean` |
| `isStatic(integer entity = getEntity()) => boolean` |
| `isSummon(integer entity = getEntity()) => boolean` |
| `listen() => Array<Array<integer|string>>` |
| `say(string message) => void` |
| `setWeapon(integer weapon) => void` |
| `canUseWeapon(integer weapon = getWeapon(), integer target) => boolean` |
| `canUseWeaponOnCell(integer weapon = getWeapon(), integer target) => boolean` |
| `getAllWeapons() => Array<integer>` |
| `getWeaponArea(integer weapon = getWeapon()) => integer` |
| `getWeaponCost(integer weapon = getWeapon()) => integer` |
| `getWeaponEffectiveArea(integer targetCell, integer fromCell) => Array<integer>` |
| `getWeaponEffectiveArea(integer weapon, integer targetCell, integer fromCell = getCell()) => Array<integer>` |
| `getWeaponEffects(integer weapon = getWeapon()) => Array<Array<integer|boolean>>` |
| `getWeaponFailureRate(integer weapon = getWeapon()) => integer` |
| `getWeaponLaunchType(integer weapon = getWeapon()) => integer` |
| `getWeaponMaxRange(integer weapon = getWeapon()) => integer` |
| `getWeaponMaxScope(integer weapon = getWeapon()) => integer` |
| `getWeaponMaxUses(integer weapon = getWeapon()) => integer` |
| `getWeaponMinRange(integer weapon = getWeapon()) => integer` |
| `getWeaponMinScope(integer weapon = getWeapon()) => integer` |
| `getWeaponName(integer weapon = getWeapon()) => string` |
| `getWeaponPassiveEffects(integer weapon = getWeapon()) => Array<Array<integer|boolean>>` |
| `isInlineWeapon(integer weapon = getWeapon()) => boolean` |
| `isWeapon(any value) => boolean` |
| `useWeapon(integer target) => integer` |
| `useWeaponOnCell(integer target) => integer` |
| `weaponNeedLos(integer weapon = getWeapon()) => boolean` |
| `canUseChip(integer chip, integer target) => boolean` |
| `canUseChipOnCell(integer chip, integer target) => boolean` |
| `chipNeedLos(integer chip) => boolean` |
| `getAllChips() => Array<integer>` |
| `getChipArea(integer chip) => integer` |
| `getChipCooldown(integer chip) => integer` |
| `getChipCost(integer chip) => integer` |
| `getChipEffectiveArea(integer chip, integer targetCell, integer fromCell = getCell()) => Array<integer>` |
| `getChipEffects(integer chip) => Array<Array<integer|boolean>>` |
| `getChipFailureRate(integer chip) => integer` |
| `getChipLaunchType(integer chip) => integer` |
| `getChipMaxRange(integer chip) => integer` |
| `getChipMaxScope(integer chip) => integer` |
| `getChipMaxUses(integer chip) => integer` |
| `getChipMinRange(integer chip) => integer` |
| `getChipMinScope(integer chip) => integer` |
| `getChipName(integer chip) => string` |
| `getCooldown(integer chip, integer entity = getEntity()) => integer` |
| `isChip(any value) => boolean` |
| `isInlineChip(integer chip) => boolean` |
| `resurrect(integer entity, integer cell) => integer` |
| `summon(integer chip, integer cell, Function<void> callback, string? name) => integer` |
| `useChip(integer chip, integer target = getEntity()) => integer` |
| `useChipOnCell(integer chip, integer cell) => integer` |
| `getCellContent(integer cell) => integer` |
| `getCellDistance(integer cellA, integer cellB) => integer` |
| `getCellFromXY(integer x, integer y) => integer` |
| `getCellX(integer cell) => integer` |
| `getCellY(integer cell) => integer` |
| `getDistance(integer cellA, integer cellB) => integer` |
| `getEntityOnCell(integer cell) => integer` |
| `getLeekOnCell(integer cell) => integer` |
| `getMapType() => integer` |
| `getObstacles() => Array<integer>` |
| `getPath(integer cellA, integer cellB, Array<integer> ignoredCells = []) => Array<integer>?` |
| `getPathLength(integer cellA, integer cellB, Array<integer> ignoredCells = []) => integer` |
| `isEmptyCell(integer cell) => boolean` |
| `isEntity(integer cell) => boolean` |
| `isLeek(integer cell) => boolean` |
| `isObstacle(integer cell) => boolean` |
| `isOnSameLine(integer cellA, integer cellB) => boolean` |
| `getAliveAllies() => Array<integer>` |
| `getAliveAlliesCount() => integer` |
| `getAliveEnemies() => Array<integer>` |
| `getAliveEnemiesCount() => integer` |
| `getEffects() => Array<integer>` |
| `getAlliedTurret() => integer?` |
| `getAllies() => Array<integer>` |
| `getAlliesCount() => integer` |
| `getAlliesLife() => integer` |
| `getBulbStats(integer chipId) => Map<integer, Array<integer>>` |
| `getBulbCharacteristics(integer chipId) => Map<integer, Array<integer>>` |
| `getBulbChips(integer chipId) => Array<integer>` |
| `getBulbType(integer entity) => integer` |
| `getCellsToUseChip(integer chip, integer target, Array<integer> ignoredCells = []) => Array<integer>` |
| `getCellsToUseChipOnCell(integer chip, integer cell, Array<integer> ignoredCells = []) => Array<integer>` |
| `getCellsToUseWeapon(integer target) => integer` |
| `getCellsToUseWeapon(integer weapon, integer target, Array<integer> ignoredCells = []) => Array<integer>` |
| `getCellsToUseWeaponOnCell(integer target) => Array<integer>` |
| `getCellsToUseWeaponOnCell(integer weapon, integer cell, Array<integer> ignoredCells = []) => Array<integer>` |
| `getCellToUseChip(integer chip, integer target, Array<integer> ignoredCells = []) => integer` |
| `getCellToUseChipOnCell(integer chip, integer cell, Array<integer> ignoredCells = []) => integer` |
| `getCellToUseWeapon(integer target) => integer` |
| `getCellToUseWeapon(integer weapon, integer target, Array<integer> ignoredCells = []) => integer` |
| `getCellToUseWeaponOnCell(integer target) => integer` |
| `getChipTargets(integer chip, integer cell) => Array<integer>` |
| `getDeadAllies() => Array<integer>` |
| `getDeadAlliesCount() => integer` |
| `getDeadEnemies() => Array<integer>` |
| `getDeadEnemiesCount() => integer` |
| `getEnemiesLife() => integer` |
| `getEnemyTurret() => integer?` |
| `getFarthestAlly() => integer` |
| `getFarthestAlly() => integer` |
| `getFightBoss() => integer` |
| `getFightContext() => integer` |
| `getFightId() => integer` |
| `getFightType() => integer` |
| `getNearestAlly() => integer` |
| `getNearestAllyTo(integer entity) => integer` |
| `getNearestAllyToCell(integer cell) => integer` |
| `getNearestEnemy() => integer` |
| `getNearestEnemyTo(integer entity) => integer` |
| `getNearestEnemyToCell(integer cell) => integer` |
| `getNextPlayer(integer entity) => integer` |
| `getPreviousPlayer(integer entity) => integer` |
| `getTurn() => integer` |
| `getWeaponTargets(integer weapon = getWeapon(), integer cell) => Array<integer>` |
| `lineOfSight(integer start, integer end, Array<integer> entitiesToIgnore = []) => boolean` |
| `moveAwayFrom(integer entity, integer mp = getMP()) => integer` |
| `moveAwayFromCell(integer cell, integer mp = getMP()) => integer` |
| `moveAwayFromCells(Array<integer> cells, integer mp = getMP()) => integer` |
| `moveAwayFromEntities(Array<integer> entities, integer mp = getMP()) => integer` |
| `moveAwayFromLeeks(Array<integer> entities, integer mp = getMP()) => integer` |
| `moveAwayFromLine(integer cellA, integer cellB, integer mp = getMP()) => integer` |
| `moveToward(integer entity, integer mp = getMP()) => integer` |
| `moveTowardCell(integer cell, integer mp = getMP()) => integer` |
| `moveTowardCells(Array<integer> cells, integer mp = getMP()) => integer` |
| `moveTowardEntities(Array<integer> entities, integer mp = getMP()) => integer` |
| `moveTowardLeeks(Array<integer> entities, integer mp = getMP()) => integer` |
| `moveTowardLine(integer cellA, integer cellB, integer mp = getMP()) => integer` |
| `clearMarks() => void` |
| `deleteRegister(string key) => void` |
| `getDate() => string` |
| `getOperations() => integer` |
| `getInstructionCount() => integer` |
| `getMaxOperations() => integer` |
| `getRegister(string key) => string?` |
| `getRegisters() => Map<string, string>` |
| `getTime() => string` |
| `getTimestamp() => integer` |
| `getRam() => integer` |
| `include(string path) => void` |
| `mark(Array<integer>|integer cells, integer color = 0, integer turns = 0) => boolean` |
| `markText(Array<integer>|integer cells, string text, integer color = 0, integer time = 0) => boolean` |
| `pause() => void` |
| `setRegister(string key, string value) => boolean` |
| `show(integer cell, integer color = 0) => boolean` |
| `getMessageAuthor(Array message) => integer` |
| `getMessageParams(Array message) => any` |
| `getMessages(integer entity = getEntity()) => Array<Array<any>>` |
| `getMessageType(Array message) => integer` |
| `sendAll(integer type, any params) => void` |
| `sendTo(integer entity, integer type, any params) => void` |


## Relation to resolution

Name resolution seeds the **stdlib global identifier list** plus **`Infinity`**, **`PI`**, **`E`**. The signature layers **MAY** declare additional **`global`** constants; those names **SHOULD** appear in the merged global set used when **type-aware checking** loads signatures.

---

*Revision: generated catalog; maintain via `gen_spec_appendices`.*
