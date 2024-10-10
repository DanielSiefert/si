import * as _ from "lodash-es";
import { defineStore } from "pinia";
// import storage from "local-storage-fallback"; // drop-in storage polyfill which falls back to cookies/memory
import { ApiRequest } from "@si/vue-lib/pinia";
import { promiseDelay } from "@si/ts-lib";
import { posthog } from "posthog-js";
import { ISODateString } from "./shared-types";

export type UserId = string;

// TODO: figure out good way to share this type with backend...
export type User = {
  id: UserId;
  auth0Id: string; // auth0 id - based on 3rd party
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  auth0Details?: any; // json blob, just store auth0 details for now
  nickname: string;
  firstName: string | null;
  lastName: string | null;
  email?: string;
  emailVerified: boolean;
  pictureUrl: string | null;
  needsTosUpdate?: boolean;
  agreedTosVersion?: string;
  githubUsername?: string;
  discordUsername?: string;
  quarantinedAt?: ISODateString;
  suspendedAt?: ISODateString;
  onboardingDetails?: {
    vroStepsCompletedAt?: Record<string, ISODateString>;
    reviewedProfile?: ISODateString;
    company?: string;
    cloudProviders?: string[];
    devOpsTools?: string[];
    openSource?: boolean;
  };
};

export type BillingDetails = {
  id: UserId;
  firstName: string | null;
  lastName: string | null;
  email: string;
  companyInformation: {
    legalName: string | null;
    legalNumber: string | null;
    taxIdentificationNumber: string | null;
    phoneNumber: string | null;
  };
  billingInformation: {
    addressLine1: string | null;
    addressLine2: string | null;
    zipCode: string | null;
    city: string | null;
    state: string | null;
    country: string | null;
  };
  customerCheckoutUrl: string;
  customerPortalUrl: string;
};

export type SuspendedUser = {
  userId: UserId;
  email: string;
  suspendedAt: ISODateString;
};

export type QuarantinedUser = {
  userId: UserId;
  email: string;
  quarantinedAt: ISODateString;
};

export type SignupUsersReport = {
  firstName?: string | null;
  lastName?: string | null;
  email: string;
  signupMethod: string;
  discordUsername?: string | null;
  githubUsername?: string | null;
  signupAt: Date | null;
};

export const useAuthStore = defineStore("auth", {
  state: () => ({
    user: null as User | null,
    billingDetail: null as BillingDetails | null,
    waitingForAccess: false,
    suspendedUsersState: [] as SuspendedUser[] | null,
    quarantinedUsersState: [] as QuarantinedUser[] | null,
    userSignupsState: [] as SignupUsersReport[] | null,
  }),
  getters: {
    // userIsLoggedIn: (state) => !!state.token,
    userIsLoggedIn: (state) => !!state.user,
    bestUserLabel: (state) => {
      if (!state.user) return "user";
      return (
        state.user.firstName ||
        state.user.nickname ||
        state.user.email ||
        "user"
      );
    },
    invitersName: (state) => {
      if (!state.user) return "user";
      return `${state.user.firstName} ${state.user.lastName}`;
    },
    // useful to keep this logic in one place
    needsProfileUpdate: () => false, // if we need to force a profile update, change the logic here
    suspendedUsers(state): SuspendedUser[] {
      return _.values(state.suspendedUsersState);
    },
    quarantinedUsers(state): QuarantinedUser[] {
      return _.values(state.quarantinedUsersState);
    },
    userSignups(state): SignupUsersReport[] {
      return _.values(state.userSignupsState);
    },
  },
  actions: {
    // fetches user + billing account info - called on page refresh
    // split from LOAD_USER since it will likely change
    // and because this request loading blocks the whole page/app
    async CHECK_AUTH() {
      return new ApiRequest<{ user: User }>({
        url: "/whoami",
        onSuccess: (response) => {
          this.user = response.user;
          posthog.identify(this.user.id);
          if (this.user.email) {
            posthog.alias(this.user.id, this.user.email);
          }
        },
        onFail(e) {
          /* eslint-disable-next-line no-console */
          console.log("RESTORE AUTH FAILED!", e);
          // trigger logout?
        },
      });
    },

    async logout() {
      posthog.reset();
      // see https://github.com/PostHog/posthog-js/issues/205
      posthog._handle_unload(); // flush the buffer
      await promiseDelay(500);
      // auth is on http secure cookie, so API is needed to log out
      // we redirect rather than using an api req so the api can also redirect us to auth0 logout url
      window.location.href = `${import.meta.env.VITE_AUTH_API_URL}/auth/logout`;
    },

    async LOAD_USER() {
      return new ApiRequest<{ user: User }>({
        url: "/whoami",
        onSuccess: (response) => {
          this.user = response.user;
        },
      });
    },
    async LOAD_BILLING_DETAILS() {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ billingDetails: BillingDetails }>({
        url: `/users/${this.user.id}/billingDetails`,
        onSuccess: (response) => {
          this.billingDetail = response.billingDetails;
        },
      });
    },
    async UPDATE_BILLING_DETAILS(billingDetail: Partial<BillingDetails>) {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ billingDetails: BillingDetails }>({
        method: "patch",
        url: `/users/${this.user.id}/billingDetails`,
        params: billingDetail,
        onSuccess: (response) => {
          this.billingDetail = response.billingDetails;
        },
      });
    },

    async UPDATE_USER(user: Partial<User>) {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ user: User }>({
        method: "patch",
        url: `/users/${this.user.id}`,
        params: user,
        onSuccess: (response) => {
          this.user = response.user;
        },
      });
    },
    async SET_USER_QUARANTINE(userId: string, isQuarantined: boolean) {
      return new ApiRequest<{ user: User }>({
        method: "patch",
        url: `/users/${userId}/quarantine`,
        params: {
          isQuarantined,
        },
        onSuccess: () => {
          // eslint-disable-next-line @typescript-eslint/no-floating-promises
          this.GET_SUSPENDED_USERS();
        },
      });
    },
    async SET_USER_SUSPENSION(userId: string, isSuspended: boolean) {
      return new ApiRequest<{ user: User }>({
        method: "patch",
        url: `/users/${userId}/suspend`,
        params: {
          isSuspended,
        },
        onSuccess: () => {
          // eslint-disable-next-line @typescript-eslint/no-floating-promises
          this.GET_SUSPENDED_USERS();
        },
      });
    },

    async GET_SUSPENDED_USERS() {
      return new ApiRequest<SuspendedUser[]>({
        url: `/users/suspended`,
        onSuccess: (response) => {
          this.suspendedUsersState = response;
        },
      });
    },

    async GET_QUARANTINED_USERS() {
      return new ApiRequest<QuarantinedUser[]>({
        url: `/users/quarantined`,
        onSuccess: (response) => {
          this.quarantinedUsersState = response;
        },
      });
    },

    async GET_USER_SIGNUP_REPORT(startDate: Date, endDate: Date) {
      return new ApiRequest<SignupUsersReport[]>({
        url: `/users/report`,
        params: {
          startDate,
          endDate,
        },
        onSuccess: (response) => {
          this.userSignupsState = response;
        },
      });
    },

    async REFRESH_AUTH0_PROFILE() {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ user: User }>({
        method: "post",
        url: `/users/${this.user.id}/refresh-auth0-profile`,
        onSuccess: (response) => {
          this.user = response.user;
        },
      });
    },
    async RESEND_EMAIL_VERIFICATION() {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ user: User }>({
        method: "post",
        url: `/users/${this.user.id}/resend-email-verification`,
        onSuccess: (response) => {
          // returns { success: true }
        },
        onFail: (response) => {
          // if we see this error, it means the backend will have updated the user already too
          // so we can optimistically update the user and refresh the user data
          if (response.kind === "EmailAlreadyVerified") {
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            this.user!.emailVerified = true;
            // eslint-disable-next-line @typescript-eslint/no-floating-promises
            this.LOAD_USER();
          }
        },
      });
    },

    // All of the questions answered in onboarding are put into an object
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    async COMPLETE_PROFILE(onboardingQuestions: Record<string, any>) {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest<{ user: User }>({
        method: "post",
        url: `/users/${this.user.id}/complete-profile`,
        params: onboardingQuestions,
        onSuccess: (response) => {
          this.user = response.user;
        },
      });
    },

    async AGREE_TOS(tosVersionId: string) {
      return new ApiRequest({
        method: "post",
        url: "/tos-agreement",
        params: {
          tosVersionId,
        },
        onSuccess: (response) => {
          if (!this.user) throw new Error("user not set");
          this.user.needsTosUpdate = false;
        },
      });
    },

    async BILLING_INTEGRATION() {
      if (!this.user) throw new Error("User not loaded");
      return new ApiRequest({
        method: "post",
        url: `/users/${this.user.id}/create-billing-integration`,
        onSuccess: () => {},
      });
    },

    // SIGNUP
  },
});
