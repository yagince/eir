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
  <v-card v-if="user">
    <v-list-item>
      <v-list-item-avatar>
        <v-img :src="user.avatar_url"/>
      </v-list-item-avatar>
      <v-list-item-content>
        <v-list-item-title v-html="user.login"/>
        <v-list-item-subtitle v-html="user.email"/>
      </v-list-item-content>
      <v-list-item-icon>
        <v-btn icon @click="openUserUrl">
          <v-icon>mdi-open-in-new</v-icon>
        </v-btn>
      </v-list-item-icon>
    </v-list-item>
  </v-card>
</v-container>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import { getModule } from 'vuex-module-decorators'
import GeneralStore from '@/store/modules/general'
import Github from '@/github'
import electron from 'electron'

@Component({
  components: {
  }
})
export default class Settings extends Vue {
  user: any | undefined
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

  openUserUrl () {
    if (this.user) {
      electron.shell.openExternal(this.user.html_url)
    }
  }
}
</script>
