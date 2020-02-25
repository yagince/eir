<template>
  <v-container>
    <v-form>
      <v-text-field
        v-model="token"
        label="Github Personal Access Token"
        hint="visit to https://github.com/settings/tokens"
        persistent-hint
        />
    </v-form>
  </v-container>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import { getModule } from 'vuex-module-decorators'
import GeneralStore from '@/store/modules/general'

@Component({
  components: {
  }
})
export default class Settings extends Vue {
  store (): GeneralStore {
    return getModule(GeneralStore, this.$store)
  }

  get token (): string {
    return this.store().token
  }

  set token (value: string) {
    this.store().setToken(value)
  }
}
</script>
