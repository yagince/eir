<template>
<v-container>
  <div class="text-center" v-if="pullRequestCount == 0">
    <v-icon x-large>fas fa-circle-notch fa-spin</v-icon>
  </div>
  <v-list v-else three-line>
      <v-list-item dense :key="pull.title" ripple link v-for="pull in pullRequests">
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
          <div>
            <v-chip label x-small :color="labelColorCode(label)" :key="label.id" v-for="label in pull.labels">
              {{ label.name }}
            </v-chip>
          </div>
        </v-list-item-content>
      </v-list-item>
  </v-list>
</v-container>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import PullRequestStore from '@/store/modules/pullRequests'
// import Github from '@/github'

@Component({
  components: {
  }
})
export default class PullRequests extends Vue {
  loading = false

  get pullRequests () {
    return PullRequestStore.pullRequests
  }

  get pullRequestCount () {
    return Object.values(this.pullRequests).length
  }

  labelColorCode (label: {color: string}): string {
    return `#${label.color}`
  }
}
</script>
