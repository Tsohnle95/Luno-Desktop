package org.gradle.accessors.dm;

import org.gradle.api.NonNullApi;
import org.gradle.api.artifacts.MinimalExternalModuleDependency;
import org.gradle.plugin.use.PluginDependency;
import org.gradle.api.artifacts.ExternalModuleDependencyBundle;
import org.gradle.api.artifacts.MutableVersionConstraint;
import org.gradle.api.provider.Provider;
import org.gradle.api.model.ObjectFactory;
import org.gradle.api.provider.ProviderFactory;
import org.gradle.api.internal.catalog.AbstractExternalDependencyFactory;
import org.gradle.api.internal.catalog.DefaultVersionCatalog;
import java.util.Map;
import org.gradle.api.internal.attributes.ImmutableAttributesFactory;
import org.gradle.api.internal.artifacts.dsl.CapabilityNotationParser;
import javax.inject.Inject;

/**
 * A catalog of dependencies accessible via the {@code libs} extension.
 */
@NonNullApi
public class LibrariesForLibs extends AbstractExternalDependencyFactory {

    private final AbstractExternalDependencyFactory owner = this;
    private final GoogleLibraryAccessors laccForGoogleLibraryAccessors = new GoogleLibraryAccessors(owner);
    private final JunitLibraryAccessors laccForJunitLibraryAccessors = new JunitLibraryAccessors(owner);
    private final MozillaLibraryAccessors laccForMozillaLibraryAccessors = new MozillaLibraryAccessors(owner);
    private final NewpipeLibraryAccessors laccForNewpipeLibraryAccessors = new NewpipeLibraryAccessors(owner);
    private final PuppycrawlLibraryAccessors laccForPuppycrawlLibraryAccessors = new PuppycrawlLibraryAccessors(owner);
    private final SquareupLibraryAccessors laccForSquareupLibraryAccessors = new SquareupLibraryAccessors(owner);
    private final VersionAccessors vaccForVersionAccessors = new VersionAccessors(providers, config);
    private final BundleAccessors baccForBundleAccessors = new BundleAccessors(objects, providers, config, attributesFactory, capabilityNotationParser);
    private final PluginAccessors paccForPluginAccessors = new PluginAccessors(providers, config);

    @Inject
    public LibrariesForLibs(DefaultVersionCatalog config, ProviderFactory providers, ObjectFactory objects, ImmutableAttributesFactory attributesFactory, CapabilityNotationParser capabilityNotationParser) {
        super(config, providers, objects, attributesFactory, capabilityNotationParser);
    }

    /**
     * Dependency provider for <b>jsoup</b> with <b>org.jsoup:jsoup</b> coordinates and
     * with version reference <b>jsoup</b>
     * <p>
     * This dependency was declared in catalog libs.versions.toml
     */
    public Provider<MinimalExternalModuleDependency> getJsoup() {
        return create("jsoup");
    }

    /**
     * Group of libraries at <b>google</b>
     */
    public GoogleLibraryAccessors getGoogle() {
        return laccForGoogleLibraryAccessors;
    }

    /**
     * Group of libraries at <b>junit</b>
     */
    public JunitLibraryAccessors getJunit() {
        return laccForJunitLibraryAccessors;
    }

    /**
     * Group of libraries at <b>mozilla</b>
     */
    public MozillaLibraryAccessors getMozilla() {
        return laccForMozillaLibraryAccessors;
    }

    /**
     * Group of libraries at <b>newpipe</b>
     */
    public NewpipeLibraryAccessors getNewpipe() {
        return laccForNewpipeLibraryAccessors;
    }

    /**
     * Group of libraries at <b>puppycrawl</b>
     */
    public PuppycrawlLibraryAccessors getPuppycrawl() {
        return laccForPuppycrawlLibraryAccessors;
    }

    /**
     * Group of libraries at <b>squareup</b>
     */
    public SquareupLibraryAccessors getSquareup() {
        return laccForSquareupLibraryAccessors;
    }

    /**
     * Group of versions at <b>versions</b>
     */
    public VersionAccessors getVersions() {
        return vaccForVersionAccessors;
    }

    /**
     * Group of bundles at <b>bundles</b>
     */
    public BundleAccessors getBundles() {
        return baccForBundleAccessors;
    }

    /**
     * Group of plugins at <b>plugins</b>
     */
    public PluginAccessors getPlugins() {
        return paccForPluginAccessors;
    }

    public static class GoogleLibraryAccessors extends SubDependencyFactory {

        public GoogleLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>gson</b> with <b>com.google.code.gson:gson</b> coordinates and
         * with version reference <b>gson</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getGson() {
            return create("google.gson");
        }

        /**
         * Dependency provider for <b>jsr305</b> with <b>com.google.code.findbugs:jsr305</b> coordinates and
         * with version reference <b>jsr305</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getJsr305() {
            return create("google.jsr305");
        }

        /**
         * Dependency provider for <b>protobuf</b> with <b>com.google.protobuf:protobuf-javalite</b> coordinates and
         * with version reference <b>protobuf.lib</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getProtobuf() {
            return create("google.protobuf");
        }

    }

    public static class JunitLibraryAccessors extends SubDependencyFactory {
        private final JunitJupiterLibraryAccessors laccForJunitJupiterLibraryAccessors = new JunitJupiterLibraryAccessors(owner);
        private final JunitPlatformLibraryAccessors laccForJunitPlatformLibraryAccessors = new JunitPlatformLibraryAccessors(owner);

        public JunitLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>bom</b> with <b>org.junit:junit-bom</b> coordinates and
         * with version reference <b>junit</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getBom() {
            return create("junit.bom");
        }

        /**
         * Group of libraries at <b>junit.jupiter</b>
         */
        public JunitJupiterLibraryAccessors getJupiter() {
            return laccForJunitJupiterLibraryAccessors;
        }

        /**
         * Group of libraries at <b>junit.platform</b>
         */
        public JunitPlatformLibraryAccessors getPlatform() {
            return laccForJunitPlatformLibraryAccessors;
        }

    }

    public static class JunitJupiterLibraryAccessors extends SubDependencyFactory {

        public JunitJupiterLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>api</b> with <b>org.junit.jupiter:junit-jupiter-api</b> coordinates and
         * with <b>no version specified</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getApi() {
            return create("junit.jupiter.api");
        }

        /**
         * Dependency provider for <b>engine</b> with <b>org.junit.jupiter:junit-jupiter-engine</b> coordinates and
         * with <b>no version specified</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getEngine() {
            return create("junit.jupiter.engine");
        }

        /**
         * Dependency provider for <b>params</b> with <b>org.junit.jupiter:junit-jupiter-params</b> coordinates and
         * with <b>no version specified</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getParams() {
            return create("junit.jupiter.params");
        }

    }

    public static class JunitPlatformLibraryAccessors extends SubDependencyFactory {

        public JunitPlatformLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>launcher</b> with <b>org.junit.platform:junit-platform-launcher</b> coordinates and
         * with <b>no version specified</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getLauncher() {
            return create("junit.platform.launcher");
        }

    }

    public static class MozillaLibraryAccessors extends SubDependencyFactory {
        private final MozillaRhinoLibraryAccessors laccForMozillaRhinoLibraryAccessors = new MozillaRhinoLibraryAccessors(owner);

        public MozillaLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Group of libraries at <b>mozilla.rhino</b>
         */
        public MozillaRhinoLibraryAccessors getRhino() {
            return laccForMozillaRhinoLibraryAccessors;
        }

    }

    public static class MozillaRhinoLibraryAccessors extends SubDependencyFactory {

        public MozillaRhinoLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>core</b> with <b>org.mozilla:rhino</b> coordinates and
         * with version reference <b>rhino</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getCore() {
            return create("mozilla.rhino.core");
        }

        /**
         * Dependency provider for <b>engine</b> with <b>org.mozilla:rhino-engine</b> coordinates and
         * with version reference <b>rhino</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getEngine() {
            return create("mozilla.rhino.engine");
        }

    }

    public static class NewpipeLibraryAccessors extends SubDependencyFactory {

        public NewpipeLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>nanojson</b> with <b>com.github.TeamNewPipe:nanojson</b> coordinates and
         * with version reference <b>teamnewpipe.nanojson</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getNanojson() {
            return create("newpipe.nanojson");
        }

    }

    public static class PuppycrawlLibraryAccessors extends SubDependencyFactory {

        public PuppycrawlLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>checkstyle</b> with <b>com.puppycrawl.tools:checkstyle</b> coordinates and
         * with version reference <b>checkstyle</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getCheckstyle() {
            return create("puppycrawl.checkstyle");
        }

    }

    public static class SquareupLibraryAccessors extends SubDependencyFactory {

        public SquareupLibraryAccessors(AbstractExternalDependencyFactory owner) { super(owner); }

        /**
         * Dependency provider for <b>okhttp</b> with <b>com.squareup.okhttp3:okhttp</b> coordinates and
         * with version reference <b>okhttp</b>
         * <p>
         * This dependency was declared in catalog libs.versions.toml
         */
        public Provider<MinimalExternalModuleDependency> getOkhttp() {
            return create("squareup.okhttp");
        }

    }

    public static class VersionAccessors extends VersionFactory  {

        private final ProtobufVersionAccessors vaccForProtobufVersionAccessors = new ProtobufVersionAccessors(providers, config);
        private final TeamnewpipeVersionAccessors vaccForTeamnewpipeVersionAccessors = new TeamnewpipeVersionAccessors(providers, config);
        public VersionAccessors(ProviderFactory providers, DefaultVersionCatalog config) { super(providers, config); }

        /**
         * Version alias <b>checkstyle</b> with value <b>12.3.0</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getCheckstyle() { return getVersion("checkstyle"); }

        /**
         * Version alias <b>gson</b> with value <b>2.14.0</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getGson() { return getVersion("gson"); }

        /**
         * Version alias <b>jsoup</b> with value <b>1.22.2</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getJsoup() { return getVersion("jsoup"); }

        /**
         * Version alias <b>jsr305</b> with value <b>3.0.2</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getJsr305() { return getVersion("jsr305"); }

        /**
         * Version alias <b>junit</b> with value <b>5.14.4</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getJunit() { return getVersion("junit"); }

        /**
         * Version alias <b>okhttp</b> with value <b>5.4.0</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getOkhttp() { return getVersion("okhttp"); }

        /**
         * Version alias <b>rhino</b> with value <b>1.8.1</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getRhino() { return getVersion("rhino"); }

        /**
         * Group of versions at <b>versions.protobuf</b>
         */
        public ProtobufVersionAccessors getProtobuf() {
            return vaccForProtobufVersionAccessors;
        }

        /**
         * Group of versions at <b>versions.teamnewpipe</b>
         */
        public TeamnewpipeVersionAccessors getTeamnewpipe() {
            return vaccForTeamnewpipeVersionAccessors;
        }

    }

    public static class ProtobufVersionAccessors extends VersionFactory  {

        public ProtobufVersionAccessors(ProviderFactory providers, DefaultVersionCatalog config) { super(providers, config); }

        /**
         * Version alias <b>protobuf.lib</b> with value <b>4.35.1</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getLib() { return getVersion("protobuf.lib"); }

        /**
         * Version alias <b>protobuf.plugin</b> with value <b>0.10.0</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getPlugin() { return getVersion("protobuf.plugin"); }

    }

    public static class TeamnewpipeVersionAccessors extends VersionFactory  {

        public TeamnewpipeVersionAccessors(ProviderFactory providers, DefaultVersionCatalog config) { super(providers, config); }

        /**
         * Version alias <b>teamnewpipe.nanojson</b> with value <b>e9d656ddb49a412a5a0a5d5ef20ca7ef09549996</b>
         * <p>
         * If the version is a rich version and cannot be represented as a
         * single version string, an empty string is returned.
         * <p>
         * This version was declared in catalog libs.versions.toml
         */
        public Provider<String> getNanojson() { return getVersion("teamnewpipe.nanojson"); }

    }

    public static class BundleAccessors extends BundleFactory {

        public BundleAccessors(ObjectFactory objects, ProviderFactory providers, DefaultVersionCatalog config, ImmutableAttributesFactory attributesFactory, CapabilityNotationParser capabilityNotationParser) { super(objects, providers, config, attributesFactory, capabilityNotationParser); }

    }

    public static class PluginAccessors extends PluginFactory {
        private final GooglePluginAccessors paccForGooglePluginAccessors = new GooglePluginAccessors(providers, config);

        public PluginAccessors(ProviderFactory providers, DefaultVersionCatalog config) { super(providers, config); }

        /**
         * Group of plugins at <b>plugins.google</b>
         */
        public GooglePluginAccessors getGoogle() {
            return paccForGooglePluginAccessors;
        }

    }

    public static class GooglePluginAccessors extends PluginFactory {

        public GooglePluginAccessors(ProviderFactory providers, DefaultVersionCatalog config) { super(providers, config); }

        /**
         * Plugin provider for <b>google.protobuf</b> with plugin id <b>com.google.protobuf</b> and
         * with version reference <b>protobuf.plugin</b>
         * <p>
         * This plugin was declared in catalog libs.versions.toml
         */
        public Provider<PluginDependency> getProtobuf() { return createPlugin("google.protobuf"); }

    }

}
