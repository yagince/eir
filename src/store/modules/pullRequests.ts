import { Module, VuexModule, Mutation, MutationAction, getModule } from 'vuex-module-decorators'
import store from '@/store'
import Github from '@/github'

type PullRequest = { [key: string]: any }

@Module({
  name: 'pulls',
  dynamic: true,
  store: store,
  preserveState: true
})
class PullRequestStore extends VuexModule {
  public pullRequests: { [key: string]: PullRequest } = {}

  @Mutation
  push (pull: PullRequest) {
    this.pullRequests = Object.assign({}, this.pullRequests, { [pull.id]: pull })
  }

  @MutationAction
  async fetch (token: string) {
    const github = new Github(token)
    const pulls = await github.pullRequests()
    const tmp: { [key: string]: PullRequest } = {}
    Object.entries(pulls).forEach((pullRequests) => {
      pullRequests[1].pulls.forEach((pull) => {
        tmp[pull.id] = pull
      })
    })
    return {
      pullRequests: tmp
    }
  }

  @Mutation
  clear () {
    this.pullRequests = {}
  }
}
export default getModule(PullRequestStore)
