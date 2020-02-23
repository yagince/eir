<template>
<v-app>
  <v-app-bar
    app
    color="dark"
    dark
    dense
    hide-on-scroll
    >
    <v-app-bar-nav-icon @click="navigation = true"></v-app-bar-nav-icon>
    <v-toolbar-title>EIR</v-toolbar-title>
  </v-app-bar>

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
    <HelloWorld/>
  </v-content>
</v-app>
</template>

<script lang="ts">
import { Component, Vue } from 'vue-property-decorator'
import HelloWorld from './components/HelloWorld.vue'

@Component({
  components: {
    HelloWorld
  }
})
export default class App extends Vue {
  navigation: bool = false
}
</script>
