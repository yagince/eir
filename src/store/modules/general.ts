import { Module, VuexModule, Mutation } from 'vuex-module-decorators'

@Module({
  name: 'GeneralStore',
  namespaced: true,
  stateFactory: true
})
export default class GeneralStore extends VuexModule {
  public token = 'TOKEN'

  @Mutation
  public setToken (val: string) {
    this.token = val
  }
}
