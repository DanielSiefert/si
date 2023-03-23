import { createApp } from "vue";
import FloatingVue from "floating-vue";
import VueKonva from "vue-konva";
import { createHead } from "@vueuse/head";

import "@si/vue-lib/tailwind/main.css";
import "@si/vue-lib/tailwind/tailwind.css";

import App from "@/App.vue";
import "./utils/posthog";
import router from "./router";
import store from "./store";

const app = createApp(App);

app.use(createHead());
app.use(router);
app.use(store);

// we attach to the #app-layout div (in AppLayout.vue) to stay within an overflow hidden div and not mess with page scrollbars
app.use(FloatingVue, { container: "#app-layout" });

// unfortunately, vue-konva only works as a global plugin, so we must register it here
// TODO: fork the lib and set it up so we can import individual components
app.use(VueKonva);

app.mount("#app");
