<template>
  <v-container>
    <v-form>
      <v-text-field
        v-model="token"
        label="Github Personal Access Token"
        hint="visit to https://github.com/settings/tokens"
        persistent-hint
        :loading="loading"
        />
    </v-form>
    <div v-if="user">
      {{ user.login }}
      {{ user.avatar_url }}
    </div>
  </v-container>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import { getModule } from 'vuex-module-decorators'
import GeneralStore from '@/store/modules/general'
import Github from '@/github'

@Component({
  components: {
  }
})
export default class Settings extends Vue {
  user: object | undefined
  loading = false

  created () {
    this.fetchUser(this.token)
  }

  store (): GeneralStore {
    return getModule(GeneralStore, this.$store)
  }

  get token (): string {
    return this.store().token
  }

  set token (value: string) {
    this.fetchUser(value)
    this.store().setToken(value)
  }

  async fetchUser (token: string) {
    this.loading = true
    this.user = undefined

    const github = new Github(token)

    try {
      this.user = await github.user()
    } catch (e) {
      console.log(e)
    } finally {
      this.loading = false
    }
  }
}
</script>
