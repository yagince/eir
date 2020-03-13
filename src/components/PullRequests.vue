<template>
<v-container>
  <div class="text-center" v-if="pullRequestCount == 0">
    <v-icon x-large>fas fa-circle-notch fa-spin</v-icon>
  </div>
  <v-list v-else three-line>
    <v-list-item dense :key="pull.title" ripple link v-for="pull in pullRequests">
      <v-badge
        :content="pull.issue.comments"
        :value="pull.issue.comments"
        overlap
        offset-x="30"
        offset-y="30"
        >
        <v-list-item-avatar>
          <v-avatar>
            <v-img :src="pull.user.avatar_url"/>
          </v-avatar>
        </v-list-item-avatar>
      </v-badge>
      <v-list-item-content>
        <v-list-item-title v-html="pull.title"/>
        <v-list-item-subtitle>
          <div>{{ labelRelativeDate(pull.created_at) }}</div>
          <div class="text--primary">
            {{ pull.head.repo.full_name }}
          </div>
          <div>
            {{ pull.body }}
          </div>
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
import moment from 'moment'

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

  labelRelativeDate (date: string): string {
    return moment(date).fromNow()
  }
}
</script>
