<template>
  <div v-if="visible" class="dq-overlay" @wheel.passive="hideRatingTooltip">
    <div class="dq-window">
      <header class="dq-header">
        <div class="dq-title">
          <i class="fa fa-database"></i>
          <span>数据查询</span>
          <span v-if="entries.length" class="dq-count">{{ entries.length }} 条</span>
        </div>
        <button type="button" class="dq-close" title="关闭" @click="close">
          <i class="fa fa-times"></i>
        </button>
      </header>

      <div class="dq-body">
        <aside :class="['dq-sidebar', { collapsed: sidebarCollapsed }]">
          <button
            type="button"
            class="dq-sidebar-toggle"
            :title="sidebarCollapsed ? '展开列表' : '收起列表'"
            @click="sidebarCollapsed = !sidebarCollapsed"
          >
            <i :class="sidebarCollapsed ? 'fa fa-angle-right' : 'fa fa-angle-left'"></i>
          </button>
          <template v-if="!sidebarCollapsed">
          <div class="dq-search">
            <i class="fa fa-search"></i>
            <input v-model="search" type="text" placeholder="按名称搜索" />
            <button v-if="search" type="button" class="dq-clear" @click="search = ''">
              <i class="fa fa-times-circle"></i>
            </button>
          </div>

          <div class="dq-filters">
            <button
              v-for="item in filters"
              :key="item.key"
              type="button"
              :class="['dq-filter', { active: typeFilter === item.key }]"
              @click="toggleTypeFilter(item.key)"
            >
              {{ item.label }}
            </button>
          </div>

          <div class="dq-result-line">
            <span>{{ filteredEntries.length }} 条结果</span>
            <span v-if="pageCount > 1">第 {{ page }}/{{ pageCount }} 页</span>
          </div>

          <div class="dq-list">
            <div v-if="loading" class="dq-loading">
              <i class="fa fa-spinner fa-pulse"></i>
              正在扫描 Mods 数据…
            </div>
            <div v-else-if="error" class="dq-error">{{ error }}</div>
            <button
              v-for="item in pagedEntries"
              :key="item.kind + ':' + item.id"
              type="button"
              :class="['dq-item', { active: selected && selected.kind === item.kind && selected.id === item.id }]"
              @click="selectEntry(item)"
            >
              <span class="dq-item-main">
                <span class="dq-item-name" v-html="item.name || item.id"></span>
              </span>
              <span class="dq-kind">{{ kindLabel(item.kind) }}</span>
            </button>
            <div v-if="!loading && !error && filteredEntries.length === 0" class="dq-empty">
              没有匹配的数据
            </div>
          </div>

          <div v-if="pageCount > 1" class="dq-pagination">
            <button type="button" :disabled="page <= 1" @click="page--">
              <i class="fa fa-angle-left"></i>
            </button>
            <span>{{ page }} / {{ pageCount }}</span>
            <button type="button" :disabled="page >= pageCount" @click="page++">
              <i class="fa fa-angle-right"></i>
            </button>
          </div>
          </template>
        </aside>

        <main class="dq-detail">
          <div v-if="detailLoading" class="dq-loading">
            <i class="fa fa-spinner fa-pulse"></i>
            正在读取详情…
          </div>
          <div v-else-if="detailError" class="dq-error">{{ detailError }}</div>
          <div v-else-if="!detail" class="dq-placeholder">
            <i class="fa fa-database"></i>
            <p>从左侧选择一条数据查看详情</p>
          </div>

          <!-- 机甲 -->
          <div v-else-if="detail.kind === 'mech'" class="mech-detail">
            <section class="mech-summary">
              <h2>{{ detail.name }}</h2>
              <div class="mech-meta-line">
                <div class="mech-variant">{{ detail.variant || '—' }}</div>
                <div class="mech-kicker">MECH / {{ detail.weightClass }} / {{ detail.tonnage }}T</div>
              </div>

              <div
                v-if="detail.hardpoints.length || (detail.jumpJets && detail.jumpJets.max)"
                class="summary-hardpoints"
              >
                <span v-if="detail.jumpJets && detail.jumpJets.max" class="summary-hardpoint">
                  喷射器 <b>{{ detail.jumpJets.used }}/{{ detail.jumpJets.max }}</b>
                </span>
                <span v-for="hp in detail.hardpoints" :key="hp.key" class="summary-hardpoint">
                  {{ hp.label }} <b>{{ hp.used }}/{{ hp.max }}</b>
                </span>
              </div>

              <div class="rating-list">
                <div
                  v-for="rating in detail.ratings"
                  :key="rating.key"
                  class="rating-row"
                  tabindex="0"
                  @mouseenter="showRatingTooltip($event, rating)"
                  @mouseleave="hideRatingTooltip"
                  @focus="showRatingTooltip($event, rating)"
                  @blur="hideRatingTooltip"
                >
                  <span class="rating-label">{{ rating.label }}</span>
                  <span class="rating-bars">
                    <i
                      v-for="n in 10"
                      :key="n"
                      :class="['rating-bar', { filled: n <= rating.value }]"
                    ></i>
                  </span>
                </div>
              </div>

              <div
                v-if="ratingTooltip"
                class="rating-tooltip"
                :style="ratingTooltipStyle"
                role="tooltip"
              >
                <div class="rating-tooltip-head">
                  <span>{{ ratingTooltip.label }}</span>
                  <span class="rating-tooltip-level">评级: <b>{{ ratingTooltip.value }}</b></span>
                </div>
                <div class="rating-tooltip-grid">
                  <div
                    v-for="item in ratingTooltip.details || []"
                    :key="item.label"
                    class="rating-tooltip-item"
                  >
                    <span>{{ item.label }}</span>
                    <b>{{ item.value }}</b>
                  </div>
                </div>
                <div v-if="ratingTooltip.description" class="rating-tooltip-desc">
                  {{ ratingTooltip.description }}
                </div>
              </div>

              <div class="dq-info-block">
                <div class="dq-info-label">常备角色</div>
                <div class="dq-info-text">{{ detail.stockRole || '—' }}</div>
              </div>

              <div class="dq-info-block">
                <div class="dq-info-label">机甲描述</div>
                <div class="dq-description" v-html="detail.description || '—'"></div>
              </div>

              <div v-if="detail.affinities && detail.affinities.length" class="dq-info-block">
                <div class="dq-info-label">可解锁亲和</div>
                <div class="affinity-list">
                  <div v-for="(affinity, index) in detail.affinities" :key="index" class="affinity-item">
                    <div class="affinity-head">
                      <b>{{ affinity.name }}</b>
                      <span>{{ affinity.missions }} 次任务</span>
                    </div>
                    <div class="affinity-desc">{{ affinity.description }}</div>
                  </div>
                </div>
              </div>
            </section>

            <section class="mech-structure" :style="mechStructureStyle">
              <div class="dq-section">
                <div class="dq-section-title">
                  <span>Chassis</span>
                </div>
                <div v-if="detail.chassis.slots.length" class="slot-grid chassis-slot-grid corner-frame">
                  <div
                    v-for="row in chassisSlotRows"
                    :key="row.key"
                    :class="['slot-row', 'item', slotColorClass(row.item), slotBackgroundClass(row.item), { striped: row.item.fixed, preinstalled: row.item.preinstalled || row.item.fixed, equipment: row.item.equipment }]"
                    :style="slotRowStyle(row)"
                  >
                    <span v-if="slotIcon(row.item)" class="slot-row-icon" :style="{ '--slot-icon': iconMask(row.item) }" aria-hidden="true"></span>
                    <span class="slot-row-name" v-html="row.item.name"></span>
                  </div>
                </div>
                <div v-else class="dq-muted">无固定底盘装备</div>
              </div>

              <div v-for="loc in orderedLocations" :key="loc.location" class="dq-section location-section">
                <div class="dq-section-title location-title">
                  <span>{{ loc.location }}</span>
                  <span
                    v-if="!detail.isBattleArmor || loc.armor.max > 0 || loc.structure.max > 0"
                    class="location-stats"
                  >
                    <span>S <b>{{ loc.structure.value }}/{{ loc.structure.max || '—' }}</b></span>
                    <span>F <b>{{ loc.armor.value }}/{{ loc.armor.max || '—' }}</b></span>
                    <span v-if="loc.rearArmor.max > 0">R <b>{{ loc.rearArmor.value }}/{{ loc.rearArmor.max }}</b></span>
                  </span>
                </div>
                <div v-if="lineHardpoints(loc).length" class="hardpoint-line">
                  <span v-for="hp in lineHardpoints(loc)" :key="hp.key" class="hardpoint-line-item">
                    {{ hp.label }} <b>{{ hp.used }}/{{ hp.max }}</b>
                  </span>
                </div>
                <div v-if="locationSlots(loc).length" class="slot-grid corner-frame">
                  <div
                    v-for="row in locationSlots(loc)"
                    :key="row.key"
                    :class="['slot-row', row.type, slotColorClass(row.item || row, row.type), slotBackgroundClass(row.item, row.type), { striped: row.stripe, preinstalled: row.item ? (row.item.preinstalled || row.item.fixed) : row.preinstalled, equipment: row.item && row.item.equipment, 'empty-special': row.type === 'hardpoint' && row.colorTag === 'Special' }]"
                    :style="slotRowStyle(row)"
                  >
                    <template v-if="row.type === 'item' || row.type === 'hardpointItem'">
                      <span v-if="slotIcon(row.item)" class="slot-row-icon" :style="{ '--slot-icon': iconMask(row.item) }" aria-hidden="true"></span>
                      <span class="slot-row-name" v-html="row.item.name"></span>
                      <span v-if="row.item.linked && row.item.linked.length" class="slot-linked">
                        占用 {{ row.item.linked.map(locationLabel).join('、') }}
                      </span>
                    </template>
                    <template v-else-if="row.type === 'hardpoint'">
                      <span v-if="slotIcon(row)" class="slot-row-icon" :style="{ '--slot-icon': iconMask(row) }" aria-hidden="true"></span>
                      <span class="slot-row-name">{{ row.label }}</span>
                      <span v-if="row.used" class="slot-row-meta">已占用</span>
                    </template>
                    <template v-else-if="row.type === 'dynamic'">
                      <span
                        v-if="row.item.dynamicShowIcon !== false && slotIcon(row.item)"
                        class="slot-row-icon"
                        :style="{ '--slot-icon': iconMask(row.item) }"
                        aria-hidden="true"
                      ></span>
                      <span class="slot-row-name" v-html="row.item.dynamicName || row.item.name"></span>
                      <span class="slot-row-meta">{{ row.item.dynamicBonus || 'movable slot' }}</span>
                    </template>
                    <span v-else class="slot-empty"></span>
                  </div>
                </div>
                <div v-else class="dq-muted">无装备</div>
              </div>
            </section>
          </div>

          <!-- 载具 -->
          <div v-else-if="detail.kind === 'vehicle'" class="vehicle-detail">
            <h2>{{ detail.name }}</h2>
            <div class="mech-meta-line">
              <div class="mech-variant">{{ detail.variant || '—' }}</div>
              <div class="mech-kicker">VEHICLE / {{ detail.weightClass }} / {{ detail.tonnage }}T</div>
            </div>
            <div class="dq-chips">
              <span v-if="detail.topSpeed" class="dq-chip">最高速度 <b>{{ detail.topSpeed }}</b></span>
              <span v-if="detail.movementType" class="dq-chip">移动类型 <b>{{ detail.movementType }}</b></span>
            </div>
            <div class="dq-info-block">
              <div class="dq-info-label">载具描述</div>
              <div class="dq-description" v-html="detail.description || '—'"></div>
            </div>
            <div class="mech-structure vehicle-sections">
            <div v-for="loc in orderedLocations" :key="loc.location" class="dq-section location-section">
              <div class="dq-section-title location-title">
                <span>{{ loc.location }}</span>
                <span class="location-stats">
                  <span>S <b>{{ loc.structure.value }}/{{ loc.structure.max || '—' }}</b></span>
                  <span>F <b>{{ loc.armor.value }}/{{ loc.armor.max || '—' }}</b></span>
                </span>
              </div>
              <div v-if="loc.items.length" class="item-list">
                <div v-for="(item, index) in loc.items" :key="index" class="slot-item">
                  <span class="slot-item-name">{{ item.name }}</span>
                  <span class="slot-item-meta">{{ item.componentType || '装备' }} · {{ item.slots }} 槽</span>
                  <span v-if="item.linked && item.linked.length" class="slot-linked">
                    占用 {{ item.linked.map(locationLabel).join('、') }}
                  </span>
                </div>
              </div>
              <div v-else class="dq-muted">无装备</div>
            </div>
            </div>
          </div>

          <!-- 武器 / 装备 -->
          <div v-else class="component-detail">
            <h2>{{ detail.name }}</h2>
            <div class="mech-meta-line">
              <div class="mech-variant">{{ detail.subtitle || detail.componentType || '—' }}</div>
              <div class="mech-kicker">{{ detail.kind === 'weapon' ? 'WEAPON' : detail.kind === 'ammo' ? 'AMMO' : 'EQUIPMENT' }}</div>
            </div>
            <div class="component-fields">
              <div v-for="field in detail.fields" :key="field.label" class="component-field">
                <span>{{ field.label }}</span>
                <b>{{ field.value }}</b>
              </div>
            </div>
            <div class="dq-info-block">
              <div class="dq-info-label">描述</div>
              <div class="dq-description" v-html="detail.description || '—'"></div>
            </div>
            <div v-if="detail.bonus && detail.bonus.length" class="dq-info-block">
              <div class="dq-info-label">特性</div>
              <div class="tag-list">
                <span v-for="(item, index) in detail.bonus" :key="index" class="dq-chip small">{{ item }}</span>
              </div>
            </div>
            <div v-if="detail.tags && detail.tags.length" class="dq-info-block">
              <div class="dq-info-label">标签</div>
              <div class="tag-list">
                <span v-for="(item, index) in detail.tags" :key="index" class="dq-tag">{{ item }}</span>
              </div>
            </div>
          </div>
        </main>
      </div>
    </div>
  </div>
</template>

<script>
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, toErrorMessage } from '@/api/tauri'

const ICON_PLACEHOLDER =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/%3E"

const KIND_LABELS = {
  mech: '机甲',
  vehicle: '载具',
  weapon: '武器',
  ammo: '弹药',
  equipment: '装备'
}

const LOCATION_ORDER = [
  'LeftArm',
  'RightArm',
  'LeftTorso',
  'RightTorso',
  'LeftLeg',
  'RightLeg',
  'Head',
  'CenterTorso',
  'Front',
  'Left',
  'Right',
  'Rear',
  'Turret'
]

const LOCATION_LABELS = {
  Head: '头部',
  LeftArm: '左臂',
  RightArm: '右臂',
  LeftTorso: '左躯',
  RightTorso: '右躯',
  LeftLeg: '左腿',
  RightLeg: '右腿',
  CenterTorso: '中央躯干',
  Front: '前部',
  Left: '左侧',
  Right: '右侧',
  Rear: '后部',
  Turret: '炮塔'
}

export default {
  name: 'DataQuery',
  props: {
    visible: { type: Boolean, default: false },
    rootPath: { type: String, default: '' }
  },
  emits: ['close'],
  data() {
    return {
      entries: [],
      search: '',
      typeFilter: null,
      page: 1,
      pageSize: 20,
      filters: [
        { key: 'mech', label: '机甲' },
        { key: 'vehicle', label: '载具' },
        { key: 'weapon', label: '武器' },
        { key: 'ammo', label: '弹药' },
        { key: 'equipment', label: '装备' }
      ],
      loading: false,
      error: '',
      loadedRoot: '',
      selected: null,
      detail: null,
      detailLoading: false,
      detailError: '',
      sidebarCollapsed: false,
      windowWidth: typeof window === 'undefined' ? 1600 : window.innerWidth,
      resolvedIcons: {},
      iconsResolving: false,
      ratingTooltip: null,
      ratingTooltipStyle: {}
    }
  },
  computed: {
    filteredEntries() {
      const keyword = this.search.trim().toLowerCase()
      return this.entries.filter((item) => {
        if (this.typeFilter && item.kind !== this.typeFilter) {
          return false
        }
        if (!keyword) {
          return true
        }
        return String(item.searchText || item.plainText || item.name || '')
          .toLowerCase()
          .includes(keyword)
      })
    },
    pageCount() {
      return Math.max(1, Math.ceil(this.filteredEntries.length / this.pageSize))
    },
    pagedEntries() {
      const start = (this.page - 1) * this.pageSize
      return this.filteredEntries.slice(start, start + this.pageSize)
    },
    chassisSlotRows() {
      const slots = this.detail && this.detail.chassis && this.detail.chassis.slots
      if (!Array.isArray(slots)) {
        return []
      }
      return slots.map((item, index) => ({
        key: `chassis-${index}`,
        span: Math.max(1, Number(item.slots) || 1),
        item
      }))
    },
    mechStructureStyle() {
      const sidebarWidth = this.sidebarCollapsed ? 0 : 280
      const available = Math.max(
        280,
        this.windowWidth - sidebarWidth - 24 - 320 - 14
      )
      const columns = Math.max(
        1,
        Math.min(5, Math.floor((available + 10) / 290))
      )
      return {
        '--mech-columns': columns,
        width: `${columns * 280 + (columns - 1) * 10}px`
      }
    },
    orderedLocations() {
      if (!this.detail || !Array.isArray(this.detail.locations)) {
        return []
      }
      if (this.detail.isBattleArmor) {
        return this.detail.locations
      }
      return [...this.detail.locations].sort((a, b) => {
        const ai = LOCATION_ORDER.indexOf(a.location)
        const bi = LOCATION_ORDER.indexOf(b.location)
        return (ai < 0 ? 999 : ai) - (bi < 0 ? 999 : bi)
      })
    },
    dynamicAssignments() {
      const result = {}
      if (!this.detail || !Array.isArray(this.detail.locations)) {
        return result
      }
      const locations = this.orderedLocations
      const free = {}
      const specialKeys = ['specialhandheld', 'special']

      for (const loc of locations) {
        const items = this.locationItems(loc)
        const inventory = items.reduce(
          (sum, item) => sum + Math.max(1, Number(item.inventorySize) || 1),
          0
        )
        // 空的手持/特殊硬点也会渲染成占位行（locationSlots 里 specialRows），
        // 它们占据该部位容量，必须在自由槽计算里一并扣除。
        let specialEmpty = 0
        for (const hp of loc.hardpoints || []) {
          if (!specialKeys.includes(hp.key)) continue
          const installed = items.filter(
            (item) => item.hardpointType === hp.key
          ).length
          const rows = Math.max(Number(hp.max) || 0, installed)
          specialEmpty += Math.max(0, rows - installed)
        }
        // reservedSlots 是装甲/结构/框架等底盘件占用的槽位，
        // 它们不在 locationItems 中，但同样属于该部位的 inventory。
        free[loc.location] = Math.max(
          0,
          (Number(loc.slots) || 0) -
            inventory -
            (Number(loc.reservedSlots) || 0) -
            specialEmpty
        )
      }

      const sources = []
      const seen = new Set()
      for (const loc of locations) {
        for (const item of this.locationItems(loc)) {
          if ((Number(item.dynamicSlots) || 0) <= 0) continue
          if (seen.has(item.id)) continue
          seen.add(item.id)
          sources.push({ item, sourceLocation: loc.location })
        }
      }
      for (const item of (this.detail.chassis && this.detail.chassis.slots) || []) {
        if ((Number(item.dynamicSlots) || 0) <= 0) continue
        if (seen.has(item.id)) continue
        seen.add(item.id)
        sources.push({
          item,
          sourceLocation: item.location || 'CenterTorso'
        })
      }

      const allocate = (location, source, item) => {
        if (!result[location]) result[location] = []
        result[location].push({
          name: item.name,
          item,
          sourceId: item.id,
          sourceLocation: source.sourceLocation,
          followSource:
            item.componentType === 'Weapon' &&
            (Number(item.slots) || 0) + (Number(item.dynamicSlots) || 0) > 7
        })
        free[location] = Math.max(0, (free[location] || 0) - 1)
      }

      const adjacent = {
        Head: 'CenterTorso',
        LeftTorso: 'CenterTorso',
        RightTorso: 'CenterTorso',
        LeftArm: 'LeftTorso',
        LeftLeg: 'LeftTorso',
        RightArm: 'RightTorso',
        RightLeg: 'RightTorso'
      }
      const locationalOrder = [
        'LeftArm',
        'RightArm',
        'Head',
        'LeftLeg',
        'RightLeg',
        'LeftTorso',
        'RightTorso',
        'CenterTorso'
      ]
      const priority = [
        'CenterTorso',
        'Head',
        'LeftTorso',
        'LeftLeg',
        'RightTorso',
        'RightLeg',
        'LeftArm',
        'RightArm'
      ]

      // InnerAdjacentOnly: 先填本部位，再填内侧相邻部位。
      for (const location of locationalOrder) {
        for (const source of sources) {
          const item = source.item
          if (!item.dynamicInnerAdjacent || source.sourceLocation !== location) {
            continue
          }
          let remaining = Number(item.dynamicSlots) || 0
          while (remaining > 0 && (free[location] || 0) > 0) {
            allocate(location, source, item)
            remaining--
          }
          const inner = adjacent[location]
          if (remaining > 0 && inner) {
            while (remaining > 0 && (free[inner] || 0) > 0) {
              allocate(inner, source, item)
              remaining--
            }
          }
        }
      }

      // 全局动态槽：武器先填满源部位，再按最大自由槽分配；
      // 非武器直接按游戏 LocationPriorityOrder 分配。
      for (const source of sources) {
        const item = source.item
        if (item.dynamicInnerAdjacent) continue
        let remaining = Number(item.dynamicSlots) || 0
        if (item.componentType === 'Weapon') {
          while (
            remaining > 0 &&
            (free[source.sourceLocation] || 0) > 0
          ) {
            allocate(source.sourceLocation, source, item)
            remaining--
          }
        }
        const targetGroups = [priority]
        for (const group of targetGroups) {
          while (remaining > 0) {
            const available = group.filter((name) => (free[name] || 0) > 0)
            if (!available.length) break
            // 默认给每个部位至少保留一个空槽；只有所有部位都只剩 1 槽时，
            // 才按游戏优先级强制占用最后一个空槽。
            const preferred = available.filter((name) => free[name] > 1)
            const pool = preferred.length ? preferred : available
            let target = ''
            if (preferred.length) {
              for (const name of pool) {
                if (
                  !target ||
                  free[name] > free[target] ||
                  (free[name] === free[target] &&
                    priority.indexOf(name) < priority.indexOf(target))
                ) {
                  target = name
                }
              }
            } else {
              for (const name of pool) {
                if (
                  !target ||
                  priority.indexOf(name) < priority.indexOf(target)
                ) {
                  target = name
                }
              }
            }
            if (!target) break
            allocate(target, source, item)
            remaining--
          }
          if (remaining <= 0) break
        }
      }
      return result
    }
  },
  watch: {
    visible(value) {
      if (value) {
        this.openQuery()
      }
    },
    search() {
      this.page = 1
    },
    typeFilter() {
      this.page = 1
    },
    page(value) {
      if (value < 1) this.page = 1
      if (value > this.pageCount) this.page = this.pageCount
    }
  },
  mounted() {
    document.addEventListener('keydown', this.onKeydown)
    window.addEventListener('resize', this.onResize)
  },
  beforeUnmount() {
    document.removeEventListener('keydown', this.onKeydown)
    window.removeEventListener('resize', this.onResize)
  },
  methods: {
    onResize() {
      this.windowWidth = window.innerWidth
    },
    slotIcon(item) {
      if (!item) return ''
      const path = item.iconPath || (item.icon ? this.resolvedIcons[item.icon] : '')
      if (path) return convertFileSrc(path)
      return item.icon ? ICON_PLACEHOLDER : ''
    },
    iconMask(item) {
      const src = this.slotIcon(item)
      return src ? `url("${src}")` : 'none'
    },
    slotRowStyle(row) {
      const style = { gridRow: 'span ' + row.span }
      const textColor = row.item && row.item.textColor
      if (textColor) {
        style['--slot-icon-color'] = textColor
      }
      return style
    },
    slotBackgroundClass(item, kind) {
      if (!item || kind === 'dynamic') return ''
      const tag = item.backgroundTag
      if (!tag) return ''
      const name = String(tag)
        .toLowerCase()
        .replace(/[^a-z0-9_-]/g, '')
      return name ? `bg-${name}` : ''
    },
    slotColorClass(item, kind) {
      const tag =
        (kind === 'dynamic' && item && item.dynamicColorTag) ||
        (item && item.colorTag)
      if (!tag) return ''
      const name = String(tag)
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '')
      return name ? `color-${name}` : ''
    },
    async resolveMissingIcons() {
      if (this.iconsResolving || !this.detail) return
      const icons = new Set()
      const collect = (items) => {
        for (const item of items || []) {
          if (item.icon && !item.iconPath && !this.resolvedIcons[item.icon]) {
            icons.add(item.icon)
          }
        }
      }
      for (const location of this.detail.locations || []) {
        collect(this.locationItems(location))
      }
      collect(this.detail.chassis && this.detail.chassis.slots)
      if (!icons.size) return

      this.iconsResolving = true
      try {
        const resolved = await api.dataQueryResolveIcons([...icons])
        Object.assign(this.resolvedIcons, resolved)
      } catch (error) {
        // 图标缺失不影响详情显示
      } finally {
        this.iconsResolving = false
      }
    },
    showRatingTooltip(event, rating) {
      const target = event.currentTarget
      if (!target || !target.getBoundingClientRect) {
        this.ratingTooltip = rating
        return
      }
      const rect = target.getBoundingClientRect()
      const width = 320
      const margin = 12
      const left = Math.min(
        Math.max(margin, rect.left + rect.width / 2 - width / 2),
        window.innerWidth - width - margin
      )
      const placeBelow = rect.top < 230
      const top = placeBelow ? rect.bottom + 8 : rect.top - 8
      this.ratingTooltip = rating
      this.ratingTooltipStyle = {
        left: `${left}px`,
        top: `${top}px`,
        transform: placeBelow ? 'none' : 'translateY(-100%)'
      }
    },
    hideRatingTooltip() {
      this.ratingTooltip = null
    },
    toggleTypeFilter(key) {
      this.typeFilter = this.typeFilter === key ? null : key
      this.page = 1
    },
    kindLabel(kind) {
      return KIND_LABELS[kind] || kind
    },
    locationLabel(name) {
      return LOCATION_LABELS[name] || name
    },
    hardpointName(hp) {
      const names = this.detail && this.detail.slotNames
      return (names && names[hp.key]) || hp.key
    },
    hardpointIcon(hp) {
      const icons = this.detail && this.detail.slotIcons
      return (icons && icons[hp.key]) || ''
    },
    hardpointColorTag(hp) {
      const colors = this.detail && this.detail.slotColors
      return (colors && colors[hp.key]) || ''
    },
    hardpointPreinstalled(hp) {
      const values = this.detail && this.detail.slotPreinstalled
      return Boolean(values && values[hp.key])
    },
    actuatorIcon(loc, key) {
      if (!loc || !this.detail || !this.detail.actuatorIcons) {
        return { icon: '', path: '' }
      }
      const group =
        loc.location === 'LeftArm' || loc.location === 'RightArm'
          ? 'Arm'
          : loc.location === 'LeftLeg' || loc.location === 'RightLeg'
            ? 'Leg'
            : loc.location === 'Head'
              ? 'Head'
              : ''
      if (!group) return { icon: '', path: '' }
      const icons = this.detail.actuatorIcons[group] || {}
      return icons[key] || { icon: '', path: '' }
    },
    lineHardpoints(loc) {
      return (loc.hardpoints || []).filter(
        (hp) => hp.key !== 'specialhandheld' && hp.key !== 'special'
      )
    },
    locationItems(loc) {
      const defaults = {
        LeftArm: [
          { key: 'shoulder', name: 'Shoulder' },
          { key: 'upper', name: 'Upper Arm' },
          { key: 'lower', name: 'Lower Arm' },
          { key: 'hand', name: 'Hand' }
        ],
        RightArm: [
          { key: 'shoulder', name: 'Shoulder' },
          { key: 'upper', name: 'Upper Arm' },
          { key: 'lower', name: 'Lower Arm' },
          { key: 'hand', name: 'Hand' }
        ],
        LeftLeg: [
          { key: 'hip', name: 'Hip' },
          { key: 'upper', name: 'Upper Leg' },
          { key: 'lower', name: 'Lower Leg' },
          { key: 'foot', name: 'Foot' }
        ],
        RightLeg: [
          { key: 'hip', name: 'Hip' },
          { key: 'upper', name: 'Upper Leg' },
          { key: 'lower', name: 'Lower Leg' },
          { key: 'foot', name: 'Foot' }
        ],
        Head: [
          { key: 'cockpit', name: 'Cockpit' },
          { key: 'lifesupporta', name: 'Life Support A' },
          { key: 'lifesupportb', name: 'Life Support B' },
          { key: 'sensors', name: 'Sensors' },
          { key: 'fcs', name: 'FCS' }
        ]
      }
      let defs = defaults[loc.location] || []
      if (!defs.length) {
        return loc.items || []
      }
      if (loc.location === 'LeftArm' || loc.location === 'RightArm') {
        const limits =
          (this.detail && this.detail.armLimits && this.detail.armLimits[loc.location]) ||
          { lower: true, hand: true }
        defs = defs.filter((def) => {
          if (def.key === 'lower') return limits.lower !== false
          if (def.key === 'hand') return limits.hand !== false
          return true
        })
      }
      const items = loc.items || []
      const itemText = items
        .map((item) => `${item.id || ''} ${item.name || ''}`.toLowerCase())
        .join(' ')
      // 只拿执行器文本判断手/肩/上臂/下臂/腿执行器，避免 HandHeld 武器误判。
      const actuatorText = items
        .filter((item) => /actuator|arm_part|leg_part/i.test(`${item.id || ''} ${item.name || ''}`))
        .map((item) => `${item.id || ''} ${item.name || ''}`.toLowerCase())
        .join(' ')
      const existing = loc.location === 'Head' ? itemText : actuatorText

      const allItems = [
        ...(this.detail.locations || []).flatMap((location) => location.items || []),
        ...((this.detail.chassis && this.detail.chassis.slots) || [])
      ]
      const headComponentPresent = (key) => {
        if (loc.location !== 'Head') return false
        return allItems.some((item) => {
          const id = String(item.id || '').toLowerCase()
          const name = String(item.name || '').toLowerCase()
          const categories = (item.customCategories || []).map((category) =>
            String(category).toLowerCase()
          )
          if (key === 'cockpit') {
            return (
              categories.includes('cockpit') ||
              ((id.includes('cockpit') || name.includes('cockpit')) &&
                !id.includes('lifesupport') &&
                !name.includes('life support'))
            )
          }
          if (key === 'lifesupporta') {
            return (
              categories.includes('lifesupporta') ||
              /lifesupporta|life support a/.test(`${id} ${name}`)
            )
          }
          if (key === 'lifesupportb') {
            return (
              categories.includes('lifesupportb') ||
              /lifesupportb|life support b/.test(`${id} ${name}`)
            )
          }
          return false
        })
      }

      const headColors = {
        cockpit: 'Cockpit',
        lifesupporta: 'LifeSupportA',
        lifesupportb: 'LifeSupportB',
        sensors: 'CockpitSensors',
        fcs: 'CockpitFCS'
      }
      const synthetic = defs
        .filter((def) => !headComponentPresent(def.key) && !existing.includes(def.key))
        .map((def) => ({
          id: `default_${def.key}_actuator`,
          name: def.name,
          componentType: 'Actuator',
          category: '',
          slots: 1,
          inventorySize: 1,
          dynamicInnerAdjacent: false,
          tonnage: 0,
          location: loc.location,
          hardpointSlot: -1,
          fixed: true,
          synthetic: true,
          preinstalled: true,
          equipment: loc.location !== 'Head',
          icon: this.actuatorIcon(loc, def.key).icon,
          iconPath: this.actuatorIcon(loc, def.key).path,
          colorTag:
            loc.location === 'Head'
              ? headColors[def.key] || ''
              : this.actuatorIcon(loc, def.key).colorTag,
          linked: [],
          details: ''
        }))
      return [...synthetic, ...(loc.items || [])]
    },
    isStripedSlot(loc, item) {
      // 动态槽的源头不画条纹，只有它分配到其他部位的那些槽位才画。
      if (Number(item.dynamicSlots) > 0) {
        return false
      }
      const id = String(item.id || '').toLowerCase()
      return (
        item.fixed === true ||
        item.componentType === 'Linked' ||
        id.startsWith('linked_')
      )
    },
    locationSlots(loc) {
      const items = this.locationItems(loc) || []
      const actuators = items.filter((item) => item.synthetic)
      const equipment = items.filter((item) => !item.synthetic)
      const specialKeys = ['specialhandheld', 'special']
      const specialItems = equipment.filter((item) =>
        specialKeys.includes(item.hardpointType)
      )
      const normalEquipment = equipment.filter(
        (item) => !specialKeys.includes(item.hardpointType)
      )
      const specialRows = []
      for (const hp of loc.hardpoints || []) {
        if (!specialKeys.includes(hp.key)) {
          continue
        }
        const matching = specialItems.filter((item) => item.hardpointType === hp.key)
        const max = Math.max(Number(hp.max) || 0, matching.length)
        for (let index = 0; index < max; index++) {
          const item = matching[index]
          specialRows.push(
            item
              ? {
                  key: `hardpoint-${hp.key}-${index}`,
                  type: 'hardpointItem',
                  span: 1,
                  item,
                  stripe: this.isStripedSlot(loc, item)
                }
              : {
                  key: `hardpoint-${hp.key}-${index}`,
                  type: 'hardpoint',
                  span: 1,
                  label: this.hardpointName(hp),
                  iconPath: this.hardpointIcon(hp),
                  colorTag: this.hardpointColorTag(hp),
                  preinstalled: this.hardpointPreinstalled(hp),
                  stripe: true,
                  used: false
                }
          )
        }
      }

      const rows = []
      let used = Number(loc.reservedSlots) || 0
      // 游戏顺序：Shoulder/Upper/Lower/Hand 等执行器 -> 手持/特殊槽位 -> 其他装备。
      for (const item of [
        ...actuators.map((entry) => ({ type: 'item', item: entry })),
        ...specialRows,
        ...normalEquipment.map((entry) => ({ type: 'item', item: entry }))
      ]) {
        const span = item.type === 'hardpoint' || item.type === 'hardpointItem'
          ? 1
          : Math.max(1, Number(item.item.slots) || 1)
        used += span
        rows.push(
          item.type === 'hardpoint' || item.type === 'hardpointItem'
            ? item
            : {
                key: `item-${rows.length}`,
                type: 'item',
                span,
                item: item.item,
                stripe: this.isStripedSlot(loc, item.item)
              }
        )
      }
      const dynamicRows = (this.dynamicAssignments && this.dynamicAssignments[loc.location]) || []
      // 超过 7 槽的武器：动态槽紧跟源头；其余动态槽排在所有已占用槽位之后。
      const followBySource = {}
      const tailRows = []
      for (const entry of dynamicRows) {
        if (!entry.followSource) {
          tailRows.push(entry)
          continue
        }
        if (!followBySource[entry.sourceId]) followBySource[entry.sourceId] = []
        followBySource[entry.sourceId].push(entry)
      }

      const orderedRows = []
      let dynamicIndex = 0
      for (const row of rows) {
        orderedRows.push(row)
        const sourceId = row.item && row.item.id
        if (!sourceId || !followBySource[sourceId]) continue
        for (const entry of followBySource[sourceId]) {
          used += 1
          orderedRows.push({
            key: `dynamic-${dynamicIndex++}`,
            type: 'dynamic',
            span: 1,
            item: entry.item,
            stripe: true
          })
        }
        delete followBySource[sourceId]
      }
      // 源头不在本部位（被挤走）的动态槽，退化为普通尾部动态槽。
      for (const entries of Object.values(followBySource)) {
        tailRows.push(...entries)
      }
      for (const entry of tailRows) {
        used += 1
        orderedRows.push({
          key: `dynamic-${dynamicIndex++}`,
          type: 'dynamic',
          span: 1,
          item: entry.item,
          stripe: true
        })
      }
      rows.length = 0
      rows.push(...orderedRows)
      const total = Math.max(Number(loc.slots) || 0, used)
      for (let index = used; index < total; index++) {
        rows.push({
          key: `empty-${index}`,
          type: 'empty',
          span: 1
        })
      }
      return rows
    },
    close() {
      this.hideRatingTooltip()
      this.$emit('close')
    },
    onKeydown(event) {
      if (event.key === 'Escape' && this.visible) {
        this.close()
      }
    },
    async openQuery() {
      if (!this.rootPath) {
        this.error = '尚未设置游戏根目录。'
        return
      }
      if (this.loadedRoot === this.rootPath && this.entries.length) {
        return
      }
      this.loading = true
      this.error = ''
      this.detail = null
      this.selected = null
      this.page = 1
      try {
        this.entries = await api.dataQueryEntries(this.rootPath)
        this.loadedRoot = this.rootPath
      } catch (error) {
        this.error = toErrorMessage(error)
      } finally {
        this.loading = false
      }
    },
    async selectEntry(item) {
      this.ratingTooltip = null
      this.selected = item
      this.detail = null
      this.detailError = ''
      this.detailLoading = true
      try {
        this.detail = await api.dataQueryDetail(this.rootPath, item.kind, item.id)
      } catch (error) {
        this.detailError = toErrorMessage(error)
      } finally {
        this.detailLoading = false
        if (this.detail) {
          this.resolveMissingIcons()
        }
      }
    }
  }
}
</script>
