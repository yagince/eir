import { Module, VuexModule, Mutation, getModule } from 'vuex-module-decorators'
import store from '@/store'

@Module({
  name: 'general',
  dynamic: true,
  store: store,
  preserveState: true
})
class GeneralStore extends VuexModule {
  public token = 'TOKEN'

  @Mutation
  public setToken (val: string) {
    this.token = val
  }
}

export default getModule(GeneralStore)
