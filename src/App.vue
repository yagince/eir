<template>
<v-app>
  <v-system-bar
    hide-on-scroll
    dense
    >
    <v-icon @click="navigation = true">mdi-dots-vertical</v-icon>
    <v-icon @click="reload()">mdi-reload</v-icon>
  </v-system-bar>

  <v-navigation-drawer
    v-model="navigation"
    absolute
    temporary
    >
    <v-list
      nav
      dense
      >
      <v-list-item-group>
        <v-list-item :to="{ name: 'home' }">
          <v-list-item-icon>
            <v-icon>mdi-home</v-icon>
          </v-list-item-icon>
          <v-list-item-title>
              Home
          </v-list-item-title>
        </v-list-item>

        <v-list-item :to="{ name: 'settings' }">
          <v-list-item-icon>
            <v-icon>mdi-account</v-icon>
          </v-list-item-icon>
          <v-list-item-title>Account</v-list-item-title>
        </v-list-item>
      </v-list-item-group>
    </v-list>
  </v-navigation-drawer>

  <v-content>
    <router-view/>
  </v-content>
</v-app>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import PullRequestStore from '@/store/modules/pullRequests'
import GeneralStore from '@/store/modules/general'
import Github from '@/github'

@Component({
  components: {
  }
})
export default class App extends Vue {
  navigation = false
  intervalId = 0
  interval = 10000
  loading = false

  mounted () {
    for (let i = 1; i < 99999; i++) {
      window.clearInterval(i)
    }

    // this.intervalId = window.setInterval(() => {
    this.reload()
    // }, this.interval)
  }

  reload () {
    console.log(PullRequestStore.pullRequests)
    PullRequestStore.fetch(GeneralStore.token)
  }
}
</script>
