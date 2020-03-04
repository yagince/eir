import Vue from 'vue'
import Router from 'vue-router'
import HelloWorld from '@/components/HelloWorld.vue'
import PullRequests from '@/components/PullRequests.vue'
import Settings from '@/components/Settings.vue'

Vue.use(Router)

export default new Router({
  mode: 'hash',
  routes: [
    {
      path: '/home',
      name: 'home',
      component: PullRequests
    },
    {
      path: '/settings',
      name: 'settings',
      component: Settings
    },
    {
      path: '*',
      redirect: { name: 'home' }
    }
  ]
})
