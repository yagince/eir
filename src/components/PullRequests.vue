<template>
<v-container>
  <v-list three-line>
    <template v-for="pullRequest in pullRequests">
      <v-list-item :key="pull.title" v-for="pull in pullRequest.pulls" ripple link>
        <v-list-item-avatar>
          <v-img :src="pull.user.avatar_url"/>
        </v-list-item-avatar>
        <v-list-item-content>
          <v-list-item-title v-html="pull.title"/>
          <v-list-item-subtitle>
            <p class="text--primary">
              {{ pull.head.repo.full_name }}
            </p>
            <p>
              {{ pull.body }}
            </p>
          </v-list-item-subtitle>
        </v-list-item-content>
      </v-list-item>
    </template>
  </v-list>
  {{ pullRequests.length }}
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
export default class PullRequests extends Vue {
  loading = false
  pullRequests: { repo: {key: string}; pulls: {key: string} }[] = []

  store (): GeneralStore {
    return getModule(GeneralStore, this.$store)
  }

  created () {
    this.fetchPullRequests()
  }

  async fetchPullRequests () {
    if (this.loading) {
      return
    }

    this.loading = true

    const github = new Github(this.store().token)
    try {
      github.pullRequests((repo, pulls) => {
        this.pullRequests.push({
          repo: repo,
          pulls: pulls
        })
      })
    } finally {
      this.loading = false
    }
  }
}
</script>
