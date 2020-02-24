<template>
<v-app>
  <v-system-bar
    dark
    hide-on-scroll
    dense
    >
    <v-icon @click="navigation = true">mdi-dots-vertical</v-icon>
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
      <v-list-item-group
        v-model="group"
        active-class="deep-purple--text text--accent-4"
        >
        <v-list-item>
          <v-list-item-icon>
            <v-icon>mdi-home</v-icon>
          </v-list-item-icon>
          <v-list-item-title>Home</v-list-item-title>
        </v-list-item>

        <v-list-item>
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

@Component({
  components: {
  }
})
export default class App extends Vue {
  navigation = false
}
</script>
