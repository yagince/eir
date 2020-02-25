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

@Component({
  components: {
  }
})
export default class Settings extends Vue {
  token = ''
}
</script>
