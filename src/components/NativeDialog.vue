<template>
  <transition name="nt-fade">
    <div
      v-if="visible"
      class="nt-modal"
      :class="[modalClass, { 'nt-modal--center': center }]"
      @mousedown.self="onMaskClick"
    >
      <div class="nt-dialog" :style="{ width: width }" role="dialog" aria-modal="true">
        <div v-if="title" class="nt-dialog__header">{{ title }}</div>
        <div v-else-if="showClose" class="nt-dialog__header nt-dialog__header--empty"></div>
        <button
          v-if="showClose"
          type="button"
          class="nt-dialog__close"
          aria-label="关闭"
          v-html="'&times;'"
          @click="close"
        ></button>
        <div class="nt-dialog__body">
          <slot></slot>
        </div>
        <div v-if="$slots.footer" class="nt-dialog__footer">
          <slot name="footer"></slot>
        </div>
      </div>
    </div>
  </transition>
</template>

<script>
export default {
  name: 'NativeDialog',
  props: {
    visible: { type: Boolean, default: false },
    title: { type: String, default: '' },
    width: { type: String, default: '600px' },
    showClose: { type: Boolean, default: true },
    closeOnClickModal: { type: Boolean, default: true },
    closeOnPressEscape: { type: Boolean, default: true },
    center: { type: Boolean, default: false },
    modalClass: { type: String, default: '' }
  },
  emits: ['update:visible'],
  watch: {
    visible(value) {
      if (value) {
        document.addEventListener('keydown', this.onKeydown)
      } else {
        document.removeEventListener('keydown', this.onKeydown)
      }
    }
  },
  mounted() {
    if (this.visible) {
      document.addEventListener('keydown', this.onKeydown)
    }
  },
  beforeUnmount() {
    document.removeEventListener('keydown', this.onKeydown)
  },
  methods: {
    close() {
      this.$emit('update:visible', false)
    },
    onMaskClick() {
      if (this.closeOnClickModal) {
        this.close()
      }
    },
    onKeydown(event) {
      if (event.keyCode === 27 && this.closeOnPressEscape) {
        this.close()
      }
    }
  }
}
</script>
